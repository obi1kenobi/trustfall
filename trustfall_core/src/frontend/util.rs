use async_graphql_parser::types::{BaseType, Type};
use async_graphql_value::Name;

pub(super) fn get_underlying_named_type(t: &Type) -> &Name {
    let mut base_type = &t.base;
    loop {
        match base_type {
            BaseType::Named(n) => return n,
            BaseType::List(l) => base_type = &l.base,
        }
    }
}
