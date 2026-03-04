use std::{borrow::Cow, fmt::Display, sync::Arc};

use pyo3::{exceptions::PyValueError, prelude::*, types::PyList};

#[derive(Debug, Clone)]
pub(crate) enum FieldValue {
    Null,
    Int64(i64),
    Uint64(u64),
    Float64(f64),
    String(Arc<str>),
    Boolean(bool),
    #[doc(hidden)]
    Enum(Arc<str>), // not used at the moment, here to ensure our repr is identical to upstream
    List(Vec<FieldValue>),
}

impl FieldValue {
    #[inline]
    pub(crate) fn is_null(&self) -> bool {
        matches!(self, FieldValue::Null)
    }

    #[inline]
    pub(crate) fn python_type_name(&self) -> Cow<'static, str> {
        match self {
            FieldValue::Null => Cow::Borrowed("None"),
            FieldValue::Int64(_) | FieldValue::Uint64(_) => Cow::Borrowed("int"),
            FieldValue::Float64(_) => Cow::Borrowed("float"),
            FieldValue::String(_) => Cow::Borrowed("str"),
            FieldValue::Boolean(_) => Cow::Borrowed("bool"),
            FieldValue::Enum(_) => Cow::Borrowed("enum"),
            FieldValue::List(list) => {
                let inner_type_name = list
                    .iter()
                    .filter(|&v| !v.is_null())
                    .map(FieldValue::python_type_name)
                    .next()
                    .unwrap_or("None".into());
                Cow::Owned(format!("list[{inner_type_name}]"))
            }
        }
    }
}

impl Display for FieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldValue::Null => write!(f, "null"),
            FieldValue::Int64(v) => write!(f, "{v}"),
            FieldValue::Uint64(v) => write!(f, "{v}"),
            FieldValue::Float64(v) => write!(f, "{v}"),
            FieldValue::String(v) => write!(f, "\"{v}\""),
            FieldValue::Boolean(v) => write!(f, "{v}"),
            FieldValue::Enum(v) => write!(f, "{v}"),
            FieldValue::List(v) => {
                write!(f, "[")?;
                let mut iter = v.iter();
                if let Some(next) = iter.next() {
                    write!(f, "{next}")?;
                }
                for elem in iter {
                    write!(f, ", {elem}")?;
                }
                write!(f, "]")
            }
        }
    }
}

impl<'py> IntoPyObject<'py> for FieldValue {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            FieldValue::Null => Ok(Option::<i64>::None.into_pyobject(py)?),
            FieldValue::Uint64(x) => Ok(x.into_pyobject(py).map(|x| x.into_any())?),
            FieldValue::Int64(x) => Ok(x.into_pyobject(py).map(|x| x.into_any())?),
            FieldValue::Float64(x) => Ok(x.into_pyobject(py).map(|x| x.into_any())?),
            FieldValue::String(x) => Ok(x.into_pyobject(py).map(|x| x.into_any())?),
            FieldValue::Boolean(x) => Ok(x.into_pyobject(py).map(|x| x.to_owned().into_any())?),
            FieldValue::Enum(_) => todo!(),
            FieldValue::List(x) => x
                .into_iter()
                .map(|v| v.into_pyobject(py))
                .collect::<Result<Vec<_>, _>>()?
                .into_pyobject(py),
        }
    }
}

impl<'a, 'py> pyo3::FromPyObject<'a, 'py> for FieldValue {
    type Error = PyErr;

    fn extract(value: Borrowed<'a, 'py, pyo3::PyAny>) -> Result<Self, Self::Error> {
        if value.is_none() {
            Ok(FieldValue::Null)
        } else if let Ok(inner) = value.extract::<bool>() {
            Ok(FieldValue::Boolean(inner))
        } else if let Ok(inner) = value.extract::<i64>() {
            Ok(FieldValue::Int64(inner))
        } else if let Ok(inner) = value.extract::<u64>() {
            Ok(FieldValue::Uint64(inner))
        } else if let Ok(inner) = value.extract::<f64>() {
            if inner.is_finite() {
                Ok(FieldValue::Float64(inner))
            } else {
                Err(PyValueError::new_err(format!(
                    "{inner} is not a valid query argument value: \
                    float values may not be NaN or infinity"
                )))
            }
        } else if let Ok(inner) = value.extract::<String>() {
            Ok(FieldValue::String(inner.into()))
        } else if let Ok(list) = value.cast::<PyList>() {
            let mut converted = Vec::with_capacity(list.len());
            for element in list.iter() {
                let value = element.extract::<FieldValue>()?;
                converted.push(value);
            }

            // Ensure all non-null items in the list are of the same type.
            let mut iter = converted.iter();
            let first_non_null = loop {
                let Some(next) = iter.next() else { break None };
                if !next.is_null() {
                    break Some(next);
                }
            };
            if let Some(first) = first_non_null {
                let expected = std::mem::discriminant(first);
                for other in iter {
                    if !other.is_null() {
                        let next_discriminant = std::mem::discriminant(other);
                        if expected != next_discriminant {
                            let first_type = first.python_type_name();
                            let other_type = other.python_type_name();
                            return Err(PyValueError::new_err(format!(
                                "Found elements of different (non-null) types in the same list, \
                                which is not allowed: {first} of type {first_type} vs \
                                {other} of type {other_type}"
                            )));
                        }
                    }
                }
            }

            Ok(FieldValue::List(converted))
        } else {
            let repr = value.repr();
            let display = repr
                .as_ref()
                .map_err(|_| ())
                .and_then(|x| x.to_str().map_err(|_| ()))
                .unwrap_or("<repr unavailable>");
            Err(PyValueError::new_err(format!(
                "Value {display} of type {} is not supported by Trustfall",
                value.get_type()
            )))
        }
    }
}

// TODO: Investigate making this just a transmute if it ever becomes a perf concern,
//       since the goal for this `FieldValue` is just to be a shim type that we can impl
//       Python traits on to work around the orphan rule.
//
//       Another option would be to move the `pyo3::FromPyObject` impl into `trustfall_core`
//       onto the upstream `FieldValue` type itself, and put it behind a feature.
impl From<FieldValue> for trustfall_core::ir::FieldValue {
    fn from(value: FieldValue) -> Self {
        match value {
            FieldValue::Null => trustfall_core::ir::FieldValue::Null,
            FieldValue::Int64(x) => trustfall_core::ir::FieldValue::Int64(x),
            FieldValue::Uint64(x) => trustfall_core::ir::FieldValue::Uint64(x),
            FieldValue::Float64(x) => trustfall_core::ir::FieldValue::Float64(x),
            FieldValue::String(x) => trustfall_core::ir::FieldValue::String(x),
            FieldValue::Boolean(x) => trustfall_core::ir::FieldValue::Boolean(x),
            FieldValue::Enum(x) => trustfall_core::ir::FieldValue::Enum(x),
            FieldValue::List(x) => trustfall_core::ir::FieldValue::List(
                x.into_iter().map(Into::into).collect::<Vec<_>>().into(),
            ),
        }
    }
}

impl From<trustfall_core::ir::FieldValue> for FieldValue {
    fn from(value: trustfall_core::ir::FieldValue) -> Self {
        match value {
            trustfall_core::ir::FieldValue::Null => FieldValue::Null,
            trustfall_core::ir::FieldValue::Int64(x) => FieldValue::Int64(x),
            trustfall_core::ir::FieldValue::Uint64(x) => FieldValue::Uint64(x),
            trustfall_core::ir::FieldValue::Float64(x) => FieldValue::Float64(x),
            trustfall_core::ir::FieldValue::String(x) => FieldValue::String(x),
            trustfall_core::ir::FieldValue::Boolean(x) => FieldValue::Boolean(x),
            trustfall_core::ir::FieldValue::Enum(x) => FieldValue::Enum(x),
            trustfall_core::ir::FieldValue::List(x) => {
                FieldValue::List(x.iter().cloned().map(Into::into).collect::<Vec<_>>())
            }
            _ => unreachable!("unhandled conversion: {value:?}"),
        }
    }
}
