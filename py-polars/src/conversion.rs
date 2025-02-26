use crate::error::PyPolarsEr;
use crate::prelude::*;
use crate::series::PySeries;
use polars::frame::row::Row;
use polars::prelude::AnyValue;
use pyo3::conversion::{FromPyObject, IntoPy};
use pyo3::prelude::*;
use pyo3::types::PySequence;
use pyo3::{PyAny, PyResult};
use std::any::Any;
use std::fmt::{Display, Formatter};

#[repr(transparent)]
pub struct Wrap<T>(pub T);

impl<T> Clone for Wrap<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Wrap(self.0.clone())
    }
}
impl<T> From<T> for Wrap<T> {
    fn from(t: T) -> Self {
        Wrap(t)
    }
}

fn get_pyseq(obj: &PyAny) -> PyResult<(&PySequence, usize)> {
    let seq = <PySequence as PyTryFrom>::try_from(obj)?;
    let len = seq.len()? as usize;
    Ok((seq, len))
}

impl<'a, T> FromPyObject<'a> for Wrap<ChunkedArray<T>>
where
    T: PyPolarsPrimitiveType,
    T::Native: FromPyObject<'a>,
{
    fn extract(obj: &'a PyAny) -> PyResult<Self> {
        let (seq, len) = get_pyseq(obj)?;
        let mut builder = PrimitiveChunkedBuilder::new("", len);

        for res in seq.iter()? {
            let item = res?;
            match item.extract::<T::Native>() {
                Ok(val) => builder.append_value(val),
                Err(_) => builder.append_null(),
            }
        }
        Ok(Wrap(builder.finish()))
    }
}

impl<'a> FromPyObject<'a> for Wrap<BooleanChunked> {
    fn extract(obj: &'a PyAny) -> PyResult<Self> {
        let (seq, len) = get_pyseq(obj)?;
        let mut builder = BooleanChunkedBuilder::new("", len);

        for res in seq.iter()? {
            let item = res?;
            match item.extract::<bool>() {
                Ok(val) => builder.append_value(val),
                Err(_) => builder.append_null(),
            }
        }
        Ok(Wrap(builder.finish()))
    }
}

impl<'a> FromPyObject<'a> for Wrap<Utf8Chunked> {
    fn extract(obj: &'a PyAny) -> PyResult<Self> {
        let (seq, len) = get_pyseq(obj)?;
        let mut builder = Utf8ChunkedBuilder::new("", len, len * 25);

        for res in seq.iter()? {
            let item = res?;
            match item.extract::<&str>() {
                Ok(val) => builder.append_value(val),
                Err(_) => builder.append_null(),
            }
        }
        Ok(Wrap(builder.finish()))
    }
}

impl<'a> FromPyObject<'a> for Wrap<NullValues> {
    fn extract(ob: &'a PyAny) -> PyResult<Self> {
        if let Ok(s) = ob.extract::<String>() {
            Ok(Wrap(NullValues::AllColumns(s)))
        } else if let Ok(s) = ob.extract::<Vec<String>>() {
            Ok(Wrap(NullValues::Columns(s)))
        } else if let Ok(s) = ob.extract::<Vec<(String, String)>>() {
            Ok(Wrap(NullValues::Named(s)))
        } else {
            Err(
                PyPolarsEr::Other("could not extract value from null_values argument".into())
                    .into(),
            )
        }
    }
}

impl IntoPy<PyObject> for Wrap<AnyValue<'_>> {
    fn into_py(self, py: Python) -> PyObject {
        match self.0 {
            AnyValue::UInt8(v) => v.into_py(py),
            AnyValue::UInt16(v) => v.into_py(py),
            AnyValue::UInt32(v) => v.into_py(py),
            AnyValue::UInt64(v) => v.into_py(py),
            AnyValue::Int8(v) => v.into_py(py),
            AnyValue::Int16(v) => v.into_py(py),
            AnyValue::Int32(v) => v.into_py(py),
            AnyValue::Int64(v) => v.into_py(py),
            AnyValue::Float32(v) => v.into_py(py),
            AnyValue::Float64(v) => v.into_py(py),
            AnyValue::Null => py.None(),
            AnyValue::Boolean(v) => v.into_py(py),
            AnyValue::Utf8(v) => v.into_py(py),
            AnyValue::Date32(v) => v.into_py(py),
            AnyValue::Date64(v) => v.into_py(py),
            AnyValue::Time64(v, _) => v.into_py(py),
            AnyValue::Duration(v, _) => v.into_py(py),
            AnyValue::List(v) => {
                let pypolars = PyModule::import(py, "polars").expect("polars installed");
                let pyseries = PySeries::new(v);
                let python_series_wrapper = pypolars
                    .getattr("wrap_s")
                    .unwrap()
                    .call1((pyseries,))
                    .unwrap();
                python_series_wrapper.into()
            }
            AnyValue::Object(v) => v.into_py(py),
        }
    }
}

impl ToPyObject for Wrap<AnyValue<'_>> {
    fn to_object(&self, py: Python) -> PyObject {
        self.clone().into_py(py)
    }
}

impl<'s> FromPyObject<'s> for Wrap<AnyValue<'s>> {
    fn extract(ob: &'s PyAny) -> PyResult<Self> {
        if let Ok(v) = ob.extract::<i64>() {
            Ok(AnyValue::Int64(v).into())
        } else if let Ok(v) = ob.extract::<f64>() {
            Ok(AnyValue::Float64(v).into())
        } else if let Ok(v) = ob.extract::<&'s str>() {
            Ok(AnyValue::Utf8(v).into())
        } else if let Ok(v) = ob.extract::<bool>() {
            Ok(AnyValue::Boolean(v).into())
        } else if let Ok(res) = ob.call_method0("timestamp") {
            // s to ms
            let v = res.extract::<f64>()? as i64;
            Ok(AnyValue::Date64(v * 1000).into())
        } else if ob.is_none() {
            Ok(AnyValue::Null.into())
        } else {
            Err(PyErr::from(PyPolarsEr::Other(format!(
                "row type not supported {:?}",
                ob
            ))))
        }
    }
}

impl<'s> FromPyObject<'s> for Wrap<Row<'s>> {
    fn extract(ob: &'s PyAny) -> PyResult<Self> {
        let vals = ob.extract::<Vec<Wrap<AnyValue<'s>>>>()?;
        let vals: Vec<AnyValue> = unsafe { std::mem::transmute(vals) };
        Ok(Wrap(Row(vals)))
    }
}

#[derive(Clone, Debug)]
pub struct ObjectValue {
    pub inner: PyObject,
}

impl Display for ObjectValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl PolarsObject for ObjectValue {
    fn type_name() -> &'static str {
        "object"
    }
}

impl From<PyObject> for ObjectValue {
    fn from(p: PyObject) -> Self {
        Self { inner: p }
    }
}

impl<'a> FromPyObject<'a> for ObjectValue {
    fn extract(ob: &'a PyAny) -> PyResult<Self> {
        let gil = Python::acquire_gil();
        let python = gil.python();
        Ok(ObjectValue {
            inner: ob.to_object(python),
        })
    }
}

/// # Safety
///
/// The caller is responsible for checking that val is Object otherwise UB
impl From<&dyn Any> for &ObjectValue {
    fn from(val: &dyn Any) -> Self {
        unsafe { &*(val as *const dyn Any as *const ObjectValue) }
    }
}

impl ToPyObject for ObjectValue {
    fn to_object(&self, _py: Python) -> PyObject {
        self.inner.clone()
    }
}

impl Default for ObjectValue {
    fn default() -> Self {
        let gil = Python::acquire_gil();
        let python = gil.python();
        ObjectValue {
            inner: python.None(),
        }
    }
}
