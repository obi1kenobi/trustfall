use std::sync::Arc;

use pyo3::{exceptions::PyTypeError, prelude::*, types::PyList};

use crate::errors::QueryArgumentsError;

// TODO: apply https://pyo3.rs/v0.22.3/conversions/traits#deriving-frompyobject-for-enums
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

impl<'py> pyo3::FromPyObject<'py> for FieldValue {
    fn extract_bound(value: &pyo3::Bound<'py, pyo3::PyAny>) -> pyo3::PyResult<Self> {
        if value.is_none() {
            Ok(FieldValue::Null)
        } else if let Ok(inner) = value.extract::<bool>() {
            Ok(FieldValue::Boolean(inner))
        } else if let Ok(inner) = value.extract::<i64>() {
            Ok(FieldValue::Int64(inner))
        } else if let Ok(inner) = value.extract::<u64>() {
            Ok(FieldValue::Uint64(inner))
        } else if let Ok(inner) = value.extract::<f64>() {
            // TODO: disallow and error on nan and infinite values
            Ok(FieldValue::Float64(inner))
        } else if let Ok(inner) = value.extract::<String>() {
            Ok(FieldValue::String(inner.into()))
        } else if let Ok(list) = value.downcast::<PyList>() {
            let converted = list.iter().map(|element| element.extract::<FieldValue>()).try_fold(
                vec![],
                |mut acc, item| {
                    if let Ok(value) = item {
                        acc.push(value);
                        Some(acc)
                    } else {
                        None
                    }
                },
            );

            // TODO: handle conversion errors properly
            if let Some(inner_values) = converted {
                Ok(FieldValue::List(inner_values))
            } else {
                Err(PyErr::new::<PyTypeError, &str>("first"))
            }
        } else {
            let repr = value.repr();
            let display = repr
                .as_ref()
                .map_err(|_| ())
                .and_then(|x| x.to_str().map_err(|_| ()))
                .unwrap_or("<repr unavailable>");
            Err(QueryArgumentsError::new_err(format!(
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
