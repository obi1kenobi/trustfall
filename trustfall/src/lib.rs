use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

/// Components needed to implement data providers.
pub mod provider {
    pub use trustfall_core::interpreter::{
        Adapter, ContextIterator, ContextOutcomeIterator, DataContext, VertexIterator,
    };
    pub use trustfall_core::ir::{EdgeParameters, Eid, Vid};

    /// Contains the [`BasicAdapter`] trait, a simpler variant of [`Adapter`].
    pub mod basic_adapter {
        pub use trustfall_core::interpreter::basic_adapter::BasicAdapter;
    }
}

/// Property values and query variables.
/// Useful both for querying and for implementing data providers.
pub use trustfall_core::ir::{FieldValue, TransparentValue};

/// Trustfall query schema.
pub use trustfall_core::schema::Schema;

/// Run a Trustfall query over the data provider specified by the given schema and adapter.
pub fn execute_query<'vertex>(
    schema: &Schema,
    adapter: Rc<RefCell<impl provider::Adapter<'vertex> + 'vertex>>,
    query: &str,
    variables: BTreeMap<impl Into<Arc<str>>, impl Into<FieldValue>>,
) -> anyhow::Result<Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>> + 'vertex>> {
    let parsed_query = trustfall_core::frontend::parse(schema, query)?;
    let vars = Arc::new(
        variables
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
    );

    Ok(trustfall_core::interpreter::execution::interpret_ir(
        adapter,
        parsed_query,
        vars,
    )?)
}
