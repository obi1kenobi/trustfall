use serde::{Deserialize, Serialize};

use crate::util::DisplayVec;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum InvalidSchemaError {
    #[error("Multiple schema errors: {0}")]
    MultipleErrors(DisplayVec<InvalidSchemaError>),

    #[error(
        "Field \"{0}\" on type \"{1}\" comes from the implementation of interface \"{2}\" \
        but the field's type {3} is not compatible with the {4} type required by that interface. \
        The expected type for this field is the {4} type required by the interface, with optional \
        additional non-null constraints."
    )]
    InvalidTypeWideningOfInheritedField(String, String, String, String, String)
}

impl From<Vec<InvalidSchemaError>> for InvalidSchemaError {
    fn from(v: Vec<InvalidSchemaError>) -> Self {
        assert!(!v.is_empty());
        if v.len() == 1 {
            v.into_iter().next().unwrap()
        } else {
            Self::MultipleErrors(DisplayVec(v))
        }
    }
}
