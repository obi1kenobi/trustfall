use std::{collections::BTreeMap, sync::Arc};

use maplit::btreemap;
use quote::quote;
use trustfall::{Schema, SchemaAdapter, TryIntoStruct};

use crate::{
    edges_creator::{prepare_call_parameters, FnCall},
    util::escaped_rust_name,
};

use super::{
    root::RustFile,
    util::{parse_import, to_lower_snake_case},
};

pub(super) fn make_entrypoints_file(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
    entrypoints_file: &mut RustFile,
    entrypoints_match_arms: &mut proc_macro2::TokenStream,
) {
    assert!(entrypoints_match_arms.is_empty());

    let query = r#"
{
    Entrypoint {
        name @output

        parameter_: parameter @fold {
            name @output
            type @output
        }
    }
}"#;
    let variables: BTreeMap<Arc<str>, String> = btreemap! {};

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
    struct ResultRow {
        name: String,
        parameter_name: Vec<String>,
        parameter_type: Vec<String>,
    }

    let mut rows: Vec<_> = trustfall::execute_query(querying_schema, adapter, query, variables)
        .expect("invalid query")
        .map(|x| x.try_into_struct::<ResultRow>().expect("invalid conversion"))
        .collect();
    rows.sort_unstable();
    for row in rows {
        let parameters: Vec<_> =
            row.parameter_name.into_iter().zip(row.parameter_type.into_iter()).collect();

        let (match_arm, entrypoint_fn) = make_entrypoint_fn(&row.name, &parameters);
        entrypoints_file.top_level_items.push(entrypoint_fn);
        entrypoints_match_arms.extend(match_arm);
    }

    entrypoints_file.external_imports.insert(parse_import("trustfall::provider::VertexIterator"));
    entrypoints_file.external_imports.insert(parse_import("trustfall::provider::ResolveInfo"));

    entrypoints_file.internal_imports.insert(parse_import("super::vertex::Vertex"));
}

fn make_entrypoint_fn(
    entrypoint: &str,
    parameters: &[(String, String)],
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let FnCall { fn_params, fn_args, fn_arg_prep } = prepare_call_parameters(
        parameters,
        |parameter_name| {
            format!("failed to find parameter '{parameter_name}' when resolving '{entrypoint}' starting vertices")
        },
    );

    let entrypoint_fn_name = escaped_rust_name(to_lower_snake_case(entrypoint));
    let ident = syn::Ident::new(&entrypoint_fn_name, proc_macro2::Span::call_site());
    let todo_msg =
        format!("implement resolving starting vertices for entrypoint edge '{entrypoint}'");

    let match_arm = if parameters.is_empty() {
        quote! {
            #entrypoint => super::entrypoints::#ident(resolve_info),
        }
    } else {
        quote! {
            #entrypoint => {
                #fn_arg_prep
                super::entrypoints::#ident(#fn_args resolve_info)
            }
        }
    };

    let resolver = quote! {
        pub(super) fn #ident<'a>(
            #fn_params
            _resolve_info: &ResolveInfo,
        ) -> VertexIterator<'a, Vertex> {
            todo!(#todo_msg)
        }
    };

    (match_arm, resolver)
}
