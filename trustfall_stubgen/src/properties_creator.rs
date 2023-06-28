use std::{collections::BTreeMap, sync::Arc};

use maplit::btreemap;
use quote::quote;
use trustfall::{Schema, SchemaAdapter, TryIntoStruct};

use super::{
    root::RustFile,
    util::{parse_import, property_resolver_fn_name},
};

pub(super) fn make_properties_file(
    querying_schema: &Schema,
    adapter: Arc<SchemaAdapter<'_>>,
    properties_file: &mut RustFile,
) {
    let query = r#"
{
    VertexType {
        name @output

        property @fold @transform(op: "count") @filter(op: ">", value: ["$zero"]) {
            properties: name @output
        }
    }
}"#;
    let variables: BTreeMap<Arc<str>, i64> = btreemap! {
        "zero".into() => 0,
    };

    #[derive(Debug, serde::Deserialize)]
    struct ResultRow {
        name: String,
        properties: Vec<String>,
    }

    let rows = trustfall::execute_query(querying_schema, adapter, query, variables)
        .expect("invalid query")
        .map(|x| {
            x.try_into_struct::<ResultRow>()
                .expect("invalid conversion")
        });
    for row in rows {
        let resolver = make_resolver_fn(&row.name, &row.properties);
        properties_file.top_level_items.push(resolver);
    }

    properties_file
        .external_imports
        .insert(parse_import("trustfall::provider::ContextIterator"));
    properties_file
        .external_imports
        .insert(parse_import("trustfall::provider::ContextOutcomeIterator"));
    properties_file
        .external_imports
        .insert(parse_import("trustfall::provider::ResolveInfo"));
    properties_file
        .external_imports
        .insert(parse_import("trustfall::FieldValue"));

    properties_file
        .internal_imports
        .insert(parse_import("super::vertex::Vertex"));
}

fn make_resolver_fn(type_name: &str, properties: &[String]) -> proc_macro2::TokenStream {
    let resolver_name = property_resolver_fn_name(type_name);
    let ident = syn::Ident::new(&resolver_name, proc_macro2::Span::call_site());
    let mut arms: proc_macro2::TokenStream = proc_macro2::TokenStream::new();

    for property_name in properties {
        let todo_msg = format!("implement property '{property_name}' in fn `{resolver_name}()`");
        arms.extend(quote! {
            #property_name => todo!(#todo_msg),
        });
    }

    let unreachable_msg =
        format!("attempted to read unexpected property '{{property_name}}' on type '{type_name}'");
    quote! {
        pub(super) fn #ident<'a>(
            contexts: ContextIterator<'a, Vertex>,
            property_name: &str,
            _resolve_info: &ResolveInfo,
        ) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
            match property_name {
                #arms
                _ => unreachable!(#unreachable_msg),
            }
        }
    }
}
