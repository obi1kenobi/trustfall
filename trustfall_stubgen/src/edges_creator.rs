use std::{collections::BTreeMap, sync::Arc};

use maplit::btreemap;
use quote::quote;
use trustfall::{Schema, SchemaAdapter, TryIntoStruct};

use super::{
    root::RustFile,
    util::{
        field_value_to_rust_type, parse_import, to_lower_snake_case, trustfall_type_to_rust_type,
        type_edge_resolver_fn_name,
    },
};

pub(super) fn make_edges_file(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
    edges_file: &mut RustFile,
) {
    let query = r#"
{
    VertexType {
        name @output

        edge @fold @transform(op: "count") @filter(op: ">", value: ["$zero"]) {
            edge_name: name @output

            parameter_: parameter @fold {
                name @output
                type @output
            }
        }
    }
}"#;
    let variables: BTreeMap<Arc<str>, i64> = btreemap! {
        "zero".into() => 0,
    };

    #[derive(Debug, serde::Deserialize)]
    struct ResultRow {
        name: String,
        edge_name: Vec<String>,
        parameter_name: Vec<Vec<String>>,
        parameter_type: Vec<Vec<String>>,
    }

    let rows = trustfall::execute_query(querying_schema, adapter, query, variables)
        .expect("invalid query")
        .map(|x| {
            x.try_into_struct::<ResultRow>()
                .expect("invalid conversion")
        });
    for row in rows {
        let edges: Vec<(String, Vec<(String, String)>)> = row
            .edge_name
            .into_iter()
            .zip(
                row.parameter_name
                    .into_iter()
                    .zip(row.parameter_type.into_iter()),
            )
            .map(|(edge, (param, ty))| (edge, param.into_iter().zip(ty.into_iter()).collect()))
            .collect();

        let (type_edge_resolver_fn, type_edge_mod) = make_type_edge_resolver(&row.name, edges);
        edges_file.top_level_items.push(type_edge_resolver_fn);
        edges_file.top_level_items.push(type_edge_mod);
    }

    edges_file
        .external_imports
        .insert(parse_import("trustfall::provider::ContextIterator"));
    edges_file
        .external_imports
        .insert(parse_import("trustfall::provider::ContextOutcomeIterator"));
    edges_file
        .external_imports
        .insert(parse_import("trustfall::provider::EdgeParameters"));
    edges_file
        .external_imports
        .insert(parse_import("trustfall::provider::ResolveEdgeInfo"));
    edges_file
        .external_imports
        .insert(parse_import("trustfall::provider::VertexIterator"));

    edges_file
        .internal_imports
        .insert(parse_import("super::vertex::Vertex"));
}

fn make_type_edge_resolver(
    type_name: &str,
    edges: Vec<(String, Vec<(String, String)>)>,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let lower_type_name = to_lower_snake_case(type_name);
    let mod_name = syn::Ident::new(&lower_type_name, proc_macro2::Span::call_site());

    let mut arms = proc_macro2::TokenStream::new();
    let mut edge_resolvers = proc_macro2::TokenStream::new();
    for (edge_name, params) in edges {
        let (arm, resolver) =
            make_edge_resolver_and_call(type_name, &edge_name, &params, &mod_name);
        arms.extend(arm);
        edge_resolvers.extend(resolver);
    }

    let type_edge_resolver_fn = type_edge_resolver_fn_name(&lower_type_name);
    let ident = syn::Ident::new(&type_edge_resolver_fn, proc_macro2::Span::call_site());
    let unreachable_msg =
        format!("attempted to resolve unexpected edge '{{edge_name}}' on type '{type_name}'");
    let type_edge_resolver = quote! {
        pub(super) fn #ident<'a>(
            contexts: ContextIterator<'a, Vertex>,
            edge_name: &str,
            parameters: &EdgeParameters,
            resolve_info: &ResolveEdgeInfo,
        ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
            match edge_name {
                #arms
                _ => unreachable!(#unreachable_msg),
            }
        }
    };

    let type_edge_mod = quote! {
        mod #mod_name {
            use trustfall::provider::{ContextIterator, ContextOutcomeIterator, ResolveEdgeInfo, VertexIterator};

            use super::super::vertex::Vertex;

            #edge_resolvers
        }
    };

    (type_edge_resolver, type_edge_mod)
}

fn make_edge_resolver_and_call(
    type_name: &str,
    edge_name: &str,
    parameters: &[(String, String)],
    mod_name: &proc_macro2::Ident,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let FnCall {
        fn_params,
        fn_args,
        fn_arg_prep,
    } = prepare_call_parameters(parameters, |parameter_name| {
        format!("failed to find parameter '{parameter_name}' for edge '{edge_name}' on type '{type_name}'")
    });

    let resolver_fn_name = to_lower_snake_case(edge_name);
    let resolver_fn_ident = syn::Ident::new(&resolver_fn_name, proc_macro2::Span::call_site());
    let todo_msg = format!("implement edge '{edge_name}' for type '{type_name}'");
    let resolver = quote! {
        pub(super) fn #resolver_fn_ident<'a>(
            contexts: ContextIterator<'a, Vertex>,
            #fn_params
            _resolve_info: &ResolveEdgeInfo,
        ) -> ContextOutcomeIterator<'a, Vertex, VertexIterator<'a, Vertex>> {
            todo!(#todo_msg)
        }
    };

    let match_arm = if parameters.is_empty() {
        quote! {
            #edge_name => #mod_name::#resolver_fn_ident(contexts, resolve_info),
        }
    } else {
        quote! {
            #edge_name => {
                #fn_arg_prep
                #mod_name::#resolver_fn_ident(contexts, #fn_args resolve_info)
            }
        }
    };

    (match_arm, resolver)
}

pub(super) struct FnCall {
    pub(super) fn_params: proc_macro2::TokenStream,
    pub(super) fn_args: proc_macro2::TokenStream,
    pub(super) fn_arg_prep: proc_macro2::TokenStream,
}

pub(super) fn prepare_call_parameters(
    parameters: &[(String, String)],
    expect_msg_fn: impl Fn(&str) -> String,
) -> FnCall {
    let mut fn_params: proc_macro2::TokenStream = proc_macro2::TokenStream::new();
    let mut fn_args: proc_macro2::TokenStream = proc_macro2::TokenStream::new();
    let mut fn_arg_prep: proc_macro2::TokenStream = proc_macro2::TokenStream::new();

    for (parameter_name, parameter_type) in parameters {
        let ident = syn::Ident::new(parameter_name, proc_macro2::Span::call_site());
        let ty = trustfall_type_to_rust_type(parameter_type);
        fn_params.extend(quote! {
            #ident: #ty,
        });
        fn_args.extend(quote! {
            #ident,
        });

        let expect_msg = expect_msg_fn(parameter_name);
        let parameter_get = quote! {
            parameters.get(#parameter_name).expect(#expect_msg)
        };
        let parameter_expr = field_value_to_rust_type(parameter_type, parameter_get);

        fn_arg_prep.extend(quote! {
            let #ident: #ty = #parameter_expr;
        });
    }

    FnCall {
        fn_params,
        fn_args,
        fn_arg_prep,
    }
}
