mod base;
mod operations;

pub use base::Type;
pub use operations::{is_argument_type_valid, NamedTypedValue};

pub(crate) use operations::{is_base_type_orderable, is_scalar_only_subtype};
