use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::Write,
    path::Path,
    sync::{Arc, OnceLock},
};

use maplit::btreemap;
use quote::quote;
use regex::Regex;
use trustfall::{Schema, SchemaAdapter, TryIntoStruct};

use crate::util::{escaped_rust_name, parse_import, to_lower_snake_case};

use super::{
    adapter_creator::make_adapter_file, edges_creator::make_edges_file,
    entrypoints_creator::make_entrypoints_file, properties_creator::make_properties_file,
};

/// Given a schema, make a Rust adapter stub for it in the given directory.
///
/// Generated code structure:
/// - adapter/mod.rs          connects everything together
/// - adapter/schema.graphql  contains the schema for the adapter
/// - adapter/adapter_impl.rs contains the adapter implementation
/// - adapter/vertex.rs       contains the vertex type definition
/// - adapter/entrypoints.rs  contains the entry points where all queries must start
/// - adapter/properties.rs   contains the property implementations
/// - adapter/edges.rs        contains the edge implementations
/// - adapter/tests.rs        contains test code
///
/// # Example
/// ```no_run
/// # use std::path::Path;
/// #
/// # use trustfall_stubgen::generate_rust_stub;
/// #
/// # fn main() {
/// let schema_text = std::fs::read_to_string("./schema.graphql").expect("failed to read schema");
/// generate_rust_stub(&schema_text, Path::new("crate/with/generated/stubs/src"))
///     .expect("stub generation failed");
/// # }
/// ```
pub fn generate_rust_stub(schema: &str, target: &Path) -> anyhow::Result<()> {
    let target_schema = Schema::parse(schema)?;

    let querying_schema =
        Schema::parse(SchemaAdapter::schema_text()).expect("schema querying schema was not valid");
    let schema_adapter = Arc::new(SchemaAdapter::new(&target_schema));

    let mut stub = AdapterStub::with_standard_mod(schema);

    let mut entrypoint_match_arms = proc_macro2::TokenStream::new();

    ensure_no_vertex_name_keyword_conflicts(&querying_schema, schema_adapter.clone());
    ensure_no_edge_name_keyword_conflicts(&querying_schema, schema_adapter.clone());

    make_vertex_file(&querying_schema, schema_adapter.clone(), &mut stub.vertex);
    make_entrypoints_file(
        &querying_schema,
        schema_adapter.clone(),
        &mut stub.entrypoints,
        &mut entrypoint_match_arms,
    );
    make_properties_file(&querying_schema, schema_adapter.clone(), &mut stub.properties);
    make_edges_file(&querying_schema, schema_adapter.clone(), &mut stub.edges);
    make_adapter_file(
        &querying_schema,
        schema_adapter.clone(),
        &mut stub.adapter_impl,
        entrypoint_match_arms,
    );
    make_tests_file(&mut stub.tests);

    stub.write_to_directory(target)
}

#[derive(Debug, Default)]
pub(crate) struct RustFile {
    pub(crate) builtin_imports: BTreeSet<Vec<String>>,
    pub(crate) internal_imports: BTreeSet<Vec<String>>,
    pub(crate) external_imports: BTreeSet<Vec<String>>,
    pub(crate) top_level_items: Vec<proc_macro2::TokenStream>,
}

impl RustFile {
    fn write_to_file(self, target: &Path) -> anyhow::Result<()> {
        let mut buffer: Vec<u8> = Vec::with_capacity(8192);

        write_import_tree(&mut buffer, &self.builtin_imports)?;
        if !self.builtin_imports.is_empty() {
            buffer.write_all("\n".as_bytes())?;
        }

        write_import_tree(&mut buffer, &self.external_imports)?;
        if !self.external_imports.is_empty() {
            buffer.write_all("\n".as_bytes())?;
        }

        write_import_tree(&mut buffer, &self.internal_imports)?;
        if !self.internal_imports.is_empty() {
            buffer.write_all("\n".as_bytes())?;
        }

        let mut item_iter = self.top_level_items.into_iter();
        let first_item = item_iter.next().expect("no items found");
        Self::pretty_print_item(&mut buffer, first_item)?;

        for item in item_iter {
            buffer.write_all("\n".as_bytes())?;
            Self::pretty_print_item(&mut buffer, item)?;
        }

        std::fs::write(target, buffer)?;

        Ok(())
    }

    /// Pretty-print an item into the buffer.
    ///
    /// First use `prettyplease`, then postprocess with a regex to further improve quality.
    /// `prettyplease` does not add blank lines between sibling items, so we add them via regex.
    fn pretty_print_item(
        buffer: &mut impl std::io::Write,
        item: proc_macro2::TokenStream,
    ) -> anyhow::Result<()> {
        static PATTERN: OnceLock<Regex> = OnceLock::new();
        let pattern =
            PATTERN.get_or_init(|| Regex::new("([^{])\n    (pub|fn|use)").expect("invalid regex"));

        let pretty_item =
            prettyplease::unparse(&syn::parse_str(&item.to_string()).expect("not valid Rust"));
        let postprocessed = pattern.replace_all(&pretty_item, "$1\n\n    $2");

        buffer.write_all(postprocessed.as_bytes())?;

        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NodeOrLeaf<'a> {
    Leaf,
    Node(BTreeMap<&'a str, NodeOrLeaf<'a>>),
}

impl<'a> NodeOrLeaf<'a> {
    fn insert(&mut self, path: &'a [String]) {
        if let Some(first) = path.first() {
            let rest = &path[1..];
            match self {
                Self::Leaf => {
                    *self = Self::Node(btreemap! {
                        "self" => Self::Leaf,
                        first.as_str() => Self::from_path(rest),
                    })
                }
                Self::Node(ref mut map) => match map.entry(first) {
                    std::collections::btree_map::Entry::Vacant(e) => {
                        e.insert(Self::from_path(rest));
                    }
                    std::collections::btree_map::Entry::Occupied(mut e) => {
                        e.get_mut().insert(rest);
                    }
                },
            }
        } else {
            match self {
                Self::Leaf => {} // self is already here
                Self::Node(ref mut map) => {
                    map.insert("self", Self::Leaf);
                }
            }
        }
    }

    fn from_path(path: &[String]) -> NodeOrLeaf<'_> {
        if let Some(first) = path.first() {
            let rest = &path[1..];
            NodeOrLeaf::Node(btreemap! {
                first.as_str() => Self::from_path(rest)
            })
        } else {
            NodeOrLeaf::Leaf
        }
    }
}

fn make_import_forest(imports: &BTreeSet<Vec<String>>) -> BTreeMap<&str, NodeOrLeaf<'_>> {
    let first_import = imports.first().expect("no imports").as_slice();
    let mut node = NodeOrLeaf::from_path(first_import);

    for import in imports.iter().skip(1) {
        node.insert(import.as_slice());
    }

    match node {
        NodeOrLeaf::Node(map) => map,
        NodeOrLeaf::Leaf => {
            unreachable!("unexpectedly got a leaf node for the top level of the import forest")
        }
    }
}

fn write_import_tree<W: std::io::Write>(
    writer: &mut W,
    imports: &BTreeSet<Vec<String>>,
) -> anyhow::Result<()> {
    if imports.is_empty() {
        return Ok(());
    }

    let forest = make_import_forest(imports);

    for (root, nodes) in forest {
        writer.write_all("use ".as_bytes())?;
        writer.write_all(root.as_bytes())?;

        write_import_subtree(writer, nodes)?;
        writer.write_all(";\n".as_bytes())?;
    }

    Ok(())
}

fn write_import_subtree<W: std::io::Write>(
    writer: &mut W,
    nodes: NodeOrLeaf<'_>,
) -> anyhow::Result<()> {
    match nodes {
        NodeOrLeaf::Leaf => {}
        NodeOrLeaf::Node(map) => {
            writer.write_all("::".as_bytes())?;

            if map.len() == 1 {
                for (root, inner) in map {
                    writer.write_all(root.as_bytes())?;
                    write_import_subtree(writer, inner)?;
                }
            } else {
                writer.write_all("{".as_bytes())?;

                let mut map_iter = map.into_iter();
                let (root, inner) = map_iter.next().expect("empty map found");
                writer.write_all(root.as_bytes())?;
                write_import_subtree(writer, inner)?;

                for (root, inner) in map_iter {
                    writer.write_all(", ".as_bytes())?;
                    writer.write_all(root.as_bytes())?;
                    write_import_subtree(writer, inner)?;
                }

                writer.write_all("}".as_bytes())?;
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct AdapterStub<'a> {
    mod_: RustFile,
    schema: &'a str,
    adapter_impl: RustFile,
    vertex: RustFile,
    entrypoints: RustFile,
    properties: RustFile,
    edges: RustFile,
    tests: RustFile,
}

impl<'a> AdapterStub<'a> {
    fn with_standard_mod(schema: &'a str) -> Self {
        let mut mod_ = RustFile::default();

        mod_.top_level_items.push(quote! {
            mod adapter_impl;
            mod vertex;
            mod entrypoints;
            mod properties;
            mod edges;
        });
        mod_.top_level_items.push(quote! {
            #[cfg(test)]
            mod tests;
        });
        mod_.top_level_items.push(quote! {
            pub use adapter_impl::Adapter;
            pub use vertex::Vertex;
        });

        Self {
            mod_,
            schema,
            adapter_impl: Default::default(),
            vertex: Default::default(),
            entrypoints: Default::default(),
            properties: Default::default(),
            edges: Default::default(),
            tests: Default::default(),
        }
    }

    fn write_to_directory(self, target: &Path) -> anyhow::Result<()> {
        let mut path_buf = target.to_path_buf();
        path_buf.push("adapter");
        std::fs::create_dir_all(&path_buf)?;

        path_buf.push("schema.graphql");
        std::fs::write(path_buf.as_path(), self.schema)?;
        path_buf.pop();

        path_buf.push("mod.rs");
        self.mod_.write_to_file(path_buf.as_path())?;
        path_buf.pop();

        path_buf.push("adapter_impl.rs");
        self.adapter_impl.write_to_file(path_buf.as_path())?;
        path_buf.pop();

        path_buf.push("vertex.rs");
        self.vertex.write_to_file(path_buf.as_path())?;
        path_buf.pop();

        path_buf.push("entrypoints.rs");
        self.entrypoints.write_to_file(path_buf.as_path())?;
        path_buf.pop();

        path_buf.push("properties.rs");
        self.properties.write_to_file(path_buf.as_path())?;
        path_buf.pop();

        path_buf.push("edges.rs");
        self.edges.write_to_file(path_buf.as_path())?;
        path_buf.pop();

        path_buf.push("tests.rs");
        self.tests.write_to_file(path_buf.as_path())?;
        path_buf.pop();

        Ok(())
    }
}

fn make_tests_file(tests_file: &mut RustFile) {
    tests_file
        .external_imports
        .insert(parse_import("trustfall::provider::check_adapter_invariants"));

    tests_file.internal_imports.insert(parse_import("super::Adapter"));

    tests_file.top_level_items.push(quote! {
        #[test]
        fn adapter_satisfies_trustfall_invariants() {
            let adapter = Adapter::new();
            let schema = Adapter::schema();
            check_adapter_invariants(schema, adapter);
        }
    });
}

fn make_vertex_file(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
    vertex_file: &mut RustFile,
) {
    let query = r#"
{
    VertexType {
        name @output
    }
}"#;
    let variables: BTreeMap<String, String> = Default::default();

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct ResultRow {
        name: String,
    }

    let mut variants = proc_macro2::TokenStream::new();
    let mut rows: Vec<_> = trustfall::execute_query(querying_schema, adapter, query, variables)
        .expect("invalid query")
        .map(|x| x.try_into_struct::<ResultRow>().expect("invalid conversion"))
        .collect();
    rows.sort_unstable();
    for row in rows {
        let name = &row.name;
        let ident = syn::Ident::new(name.as_str(), proc_macro2::Span::call_site());
        variants.extend(quote! {
            #ident(()),
        });
    }

    let vertex = quote! {
        #[non_exhaustive]
        #[derive(Debug, Clone, trustfall::provider::TrustfallEnumVertex)]
        pub enum Vertex {
            #variants
        }
    };

    vertex_file.top_level_items.push(vertex);
}

fn ensure_no_vertex_name_keyword_conflicts(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
) {
    let query = r#"
{
    VertexType {
        name @output
    }
}"#;
    let variables: BTreeMap<String, String> = Default::default();

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct ResultRow {
        name: String,
    }

    let mut rows: Vec<_> = trustfall::execute_query(querying_schema, adapter, query, variables)
        .expect("invalid query")
        .map(|x| {
            x.try_into_struct::<ResultRow>()
                .expect("invalid conversion")
        })
        .collect();
    rows.sort_unstable();

    let mut uniq: HashMap<String, String> = HashMap::new();

    for row in rows {
        let name = row.name.clone();
        let converted = escaped_rust_name(to_lower_snake_case(&name));
        let v = uniq.insert(converted, name);
        if let Some(v) = v {
            panic!(
                "cannot generate adapter for a schema containing both '{}' and '{}' vertices, consider renaming one of them",
                v, &row.name
            );
        }
    }
}

fn ensure_no_edge_name_keyword_conflicts(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
) {
    let query = r#"
{
    VertexType {
        name @output
        edge_: edge @fold {
            names: name @output
        }
        property_: property @fold {
            names: name @output
        }
    }
}"#;
    let variables: BTreeMap<String, String> = Default::default();

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct ResultRow {
        name: String,
        edge_names: Vec<String>,
        property_names: Vec<String>,
    }

    let mut rows: Vec<_> = trustfall::execute_query(querying_schema, adapter, query, variables)
        .expect("invalid query")
        .map(|x| {
            x.try_into_struct::<ResultRow>()
                .expect("invalid conversion")
        })
        .collect();
    rows.sort_unstable();

    for row in &rows {
        let mut uniq: HashMap<String, String> = HashMap::new();

        for edge_name in &row.edge_names {
            let edge_name_for_map = edge_name.clone();
            let converted = escaped_rust_name(to_lower_snake_case(&edge_name_for_map));
            let v = uniq.insert(converted, edge_name_for_map);
            if let Some(v) = v {
                panic!(
                    "cannot generate adapter for a schema containing both '{}' and '{}' as field names on vertex '{}', consider renaming one of them",
                    v, &edge_name, &row.name
                );
            }
        }
        for property_name in &row.property_names {
            let property_name_for_map = property_name.clone();
            let converted = escaped_rust_name(to_lower_snake_case(&property_name_for_map));
            let v = uniq.insert(converted, property_name_for_map);
            if let Some(v) = v {
                panic!(
                    "cannot generate adapter for a schema containing both '{}' and '{}' as field names on vertex '{}', consider renaming one of them",
                    v, &property_name, &row.name
                );
            }
        }
    }
}
