use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use maplit::btreemap;
use quote::quote;
use trustfall::{Schema, SchemaAdapter, TryIntoStruct};

use super::{
    root::RustFile,
    util::{parse_import, property_resolver_fn_name, type_edge_resolver_fn_name},
};

pub(super) fn make_adapter_file(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
    adapter_file: &mut RustFile,
    entrypoint_match_arms: proc_macro2::TokenStream,
) {
    let static_defn = quote! {
        static SCHEMA: OnceLock<Schema> = OnceLock::new();
    };

    let adapter_defn = quote! {
        #[derive(Debug)]
        pub struct Adapter {}
    };

    adapter_file
        .builtin_imports
        .insert(parse_import("std::sync::OnceLock"));
    adapter_file
        .external_imports
        .insert(parse_import("trustfall::Schema"));

    adapter_file
        .internal_imports
        .insert(parse_import("super::vertex::Vertex"));

    let adapter_impl = quote! {
        impl Adapter {
            pub const SCHEMA_TEXT: &'static str = include_str!("./schema.graphql");

            pub fn schema() -> &'static Schema {
                SCHEMA.get_or_init(|| Schema::parse(Self::SCHEMA_TEXT).expect("not a valid schema"))
            }
        }
    };

    let entrypoint_resolver_fn = emit_entrypoint_handling(
        entrypoint_match_arms,
        &mut adapter_file.builtin_imports,
        &mut adapter_file.external_imports,
    );
    let property_resolver_fn = emit_property_handling(
        querying_schema,
        adapter.clone(),
        &mut adapter_file.builtin_imports,
        &mut adapter_file.external_imports,
    );
    let edge_resolver_fn = emit_edge_handling(
        querying_schema,
        adapter.clone(),
        &mut adapter_file.builtin_imports,
        &mut adapter_file.external_imports,
    );
    let coercion_resolver_fn = emit_coercion_handling(
        &mut adapter_file.builtin_imports,
        &mut adapter_file.external_imports,
    );

    let trustfall_adapter_impl = quote! {
        impl<'a> trustfall::provider::Adapter<'a> for Adapter {
            type Vertex = Vertex;

            #entrypoint_resolver_fn

            #property_resolver_fn

            #edge_resolver_fn

            #coercion_resolver_fn
        }
    };

    adapter_file.top_level_items.extend([
        static_defn,
        adapter_defn,
        adapter_impl,
        trustfall_adapter_impl,
    ]);
}

fn emit_entrypoint_handling(
    entrypoint_match_arms: proc_macro2::TokenStream,
    builtin_imports: &mut BTreeSet<Vec<String>>,
    external_imports: &mut BTreeSet<Vec<String>>,
) -> proc_macro2::TokenStream {
    builtin_imports.insert(parse_import("std::sync::Arc"));
    external_imports.insert(parse_import("trustfall::provider::VertexIterator"));
    external_imports.insert(parse_import("trustfall::provider::EdgeParameters"));
    external_imports.insert(parse_import("trustfall::provider::ResolveEdgeInfo"));

    quote! {
        fn resolve_starting_vertices(
            &self,
            edge_name: &Arc<str>,
            parameters: &EdgeParameters,
            resolve_info: &ResolveInfo,
        ) -> VertexIterator<'a, Self::Vertex> {
            match edge_name.as_ref() {
                #entrypoint_match_arms
                _ => unreachable!("attempted to resolve starting vertices for unexpected edge name: {edge_name}"),
            }
        }
    }
}

fn emit_property_handling(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
    builtin_imports: &mut BTreeSet<Vec<String>>,
    external_imports: &mut BTreeSet<Vec<String>>,
) -> proc_macro2::TokenStream {
    let query = r#"
{
    VertexType {
        name @output

        property @fold @transform(op: "count") @filter(op: ">", value: ["$zero"])
    }
}"#;
    let variables: BTreeMap<Arc<str>, i64> = btreemap! {
        "zero".into() => 0,
    };

    #[derive(Debug, serde::Deserialize)]
    struct ResultRow {
        name: String,
    }

    let mut arms = proc_macro2::TokenStream::new();
    let rows = trustfall::execute_query(querying_schema, adapter, query, variables)
        .expect("invalid query")
        .map(|x| {
            x.try_into_struct::<ResultRow>()
                .expect("invalid conversion")
        });
    for row in rows {
        let name = &row.name;
        let ident = syn::Ident::new(
            &property_resolver_fn_name(name),
            proc_macro2::Span::call_site(),
        );
        arms.extend(quote! {
            #name => super::properties::#ident(contexts, property_name.as_ref(), resolve_info),
        });
    }

    builtin_imports.insert(parse_import("std::sync::Arc"));
    external_imports.insert(parse_import("trustfall::provider::ContextIterator"));
    external_imports.insert(parse_import("trustfall::provider::ContextOutcomeIterator"));
    external_imports.insert(parse_import("trustfall::provider::ResolveInfo"));
    external_imports.insert(parse_import("trustfall::FieldValue"));

    quote! {
        fn resolve_property(
            &self,
            contexts: ContextIterator<'a, Self::Vertex>,
            type_name: &Arc<str>,
            property_name: &Arc<str>,
            resolve_info: &ResolveInfo,
        ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
            match type_name.as_ref() {
                #arms
                _ => unreachable!("attempted to read property '{property_name}' on unexpected type: {type_name}"),
            }
        }
    }
}

fn emit_edge_handling(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
    builtin_imports: &mut BTreeSet<Vec<String>>,
    external_imports: &mut BTreeSet<Vec<String>>,
) -> proc_macro2::TokenStream {
    let query = r#"
{
    VertexType {
        name @output

        edge @fold @transform(op: "count") @filter(op: ">", value: ["$zero"])
    }
}"#;
    let variables: BTreeMap<Arc<str>, i64> = btreemap! {
        "zero".into() => 0,
    };

    #[derive(Debug, serde::Deserialize)]
    struct ResultRow {
        name: String,
    }

    let mut arms = proc_macro2::TokenStream::new();
    let rows = trustfall::execute_query(querying_schema, adapter, query, variables)
        .expect("invalid query")
        .map(|x| {
            x.try_into_struct::<ResultRow>()
                .expect("invalid conversion")
        });
    for row in rows {
        let name = &row.name;
        let ident = syn::Ident::new(
            &type_edge_resolver_fn_name(name),
            proc_macro2::Span::call_site(),
        );
        arms.extend(quote! {
            #name => super::edges::#ident(contexts, edge_name.as_ref(), parameters, resolve_info),
        });
    }

    builtin_imports.insert(parse_import("std::sync::Arc"));
    external_imports.insert(parse_import("trustfall::provider::ContextIterator"));
    external_imports.insert(parse_import("trustfall::provider::ContextOutcomeIterator"));
    external_imports.insert(parse_import("trustfall::provider::EdgeParameters"));
    external_imports.insert(parse_import("trustfall::provider::ResolveEdgeInfo"));
    external_imports.insert(parse_import("trustfall::provider::VertexIterator"));
    external_imports.insert(parse_import("trustfall::FieldValue"));

    quote! {
        fn resolve_neighbors(
            &self,
            contexts: ContextIterator<'a, Self::Vertex>,
            type_name: &Arc<str>,
            edge_name: &Arc<str>,
            parameters: &EdgeParameters,
            resolve_info: &ResolveEdgeInfo,
        ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>> {
            match type_name.as_ref() {
                #arms
                _ => unreachable!("attempted to resolve edge '{edge_name}' on unexpected type: {type_name}"),
            }
        }
    }
}

fn emit_coercion_handling(
    builtin_imports: &mut BTreeSet<Vec<String>>,
    external_imports: &mut BTreeSet<Vec<String>>,
) -> proc_macro2::TokenStream {
    builtin_imports.insert(parse_import("std::sync::Arc"));
    external_imports.insert(parse_import("trustfall::provider::ContextIterator"));
    external_imports.insert(parse_import("trustfall::provider::ContextOutcomeIterator"));
    external_imports.insert(parse_import("trustfall::provider::ResolveInfo"));
    external_imports.insert(parse_import(
        "trustfall::provider::resolve_coercion_using_schema",
    ));

    quote! {
        fn resolve_coercion(
            &self,
            contexts: ContextIterator<'a, Self::Vertex>,
            _type_name: &Arc<str>,
            coerce_to_type: &Arc<str>,
            _resolve_info: &ResolveInfo,
        ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
            resolve_coercion_using_schema(contexts, Self::schema(), coerce_to_type.as_ref())
        }
    }
}
