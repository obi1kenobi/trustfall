// Do NOT import FieldValue, we want to check that accessor macros does what
// we want

struct Adapter;

#[derive(Debug, Clone, trustfall::provider::TrustfallEnumVertex)]
enum V {
    Variant(Value),
}

#[derive(Debug, Clone)]
struct Value {
    name: String,
}
impl Value {
    fn initial(&self) -> String {
        self.name.chars().next().map(String::from).unwrap_or_default()
    }
}

impl trustfall::provider::BasicAdapter<'static> for Adapter {
    type Vertex = V;

    fn resolve_starting_vertices(
        &self,
        _edge_name: &str,
        _parameters: &trustfall::provider::EdgeParameters,
    ) -> trustfall::provider::VertexIterator<'static, Self::Vertex> {
        Box::new(std::iter::once(V::Variant(Value { name: String::from("Berit") })))
    }
    fn resolve_property(
        &self,
        contexts: trustfall::provider::ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        property_name: &str,
    ) -> trustfall::provider::ContextOutcomeIterator<'static, Self::Vertex, trustfall::FieldValue>
    {
        match (type_name, property_name) {
            ("Vertex", "name") => trustfall::provider::resolve_property_with(
                contexts,
                trustfall::provider::field_property!(as_variant, name),
            ),
            ("Vertex", "initial") => trustfall::provider::resolve_property_with(
                contexts,
                trustfall::provider::accessor_property!(as_variant, initial),
            ),
            (t, p) => unreachable!("tried to resolve ({t}, {p})"),
        }
    }

    fn resolve_neighbors(
        &self,
        _contexts: trustfall::provider::ContextIterator<'static, Self::Vertex>,
        _type_name: &str,
        _edge_name: &str,
        _parameters: &trustfall::provider::EdgeParameters,
    ) -> trustfall::provider::ContextOutcomeIterator<
        'static,
        Self::Vertex,
        trustfall::provider::VertexIterator<'static, Self::Vertex>,
    > {
        todo!("schema should not contain neighbors: Berit likes it that way")
    }

    fn resolve_coercion(
        &self,
        _contexts: trustfall::provider::ContextIterator<'static, Self::Vertex>,
        _type_name: &str,
        _coerce_to_type: &str,
    ) -> trustfall::provider::ContextOutcomeIterator<'static, Self::Vertex, bool> {
        todo!("there is only ever one Berit")
    }
}

#[test]
fn main() {
    let adapter = std::sync::Arc::new(Adapter);
    let schema = trustfall::Schema::parse(
        "\
schema {
    query: RootSchemaQuery
}
directive @filter(op: String!, value: [String!]) repeatable on FIELD | INLINE_FRAGMENT
directive @tag(name: String) on FIELD
directive @output(name: String) on FIELD
directive @optional on FIELD
directive @recurse(depth: Int!) on FIELD
directive @fold on FIELD
directive @transform(op: String!) on FIELD

type RootSchemaQuery {
    Person: Vertex!
}

type Vertex {
    name: String!
    initial: String!
}",
    )
    .unwrap();

    let query = "\
{
    Person {
        name @output
        initial @output
    }
}";
    let variables: std::collections::BTreeMap<std::sync::Arc<str>, trustfall::FieldValue> =
        std::collections::BTreeMap::new();
    let res = trustfall::execute_query(&schema, adapter, query, variables)
        .expect("query should resolve")
        .collect::<Vec<_>>();

    assert_eq!(res.len(), 1);

    assert_eq!(
        res[0].get("name").unwrap().to_owned(),
        trustfall::FieldValue::String("Berit".into())
    );
    assert_eq!(
        res[0].get("initial").unwrap().to_owned(),
        trustfall::FieldValue::String("B".into())
    );
}
