extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use syn::{parse_macro_input, ItemFn, LitStr};

use std::path::PathBuf;

use globset::GlobBuilder;
use quote::quote;
use walkdir::WalkDir;

#[proc_macro_attribute]
pub fn parameterize(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut result = TokenStream::new();
    result.extend(item.clone()); // emit the function itself

    let path_literal = parse_macro_input!(attr as LitStr);
    let path = path_literal.value();

    let item_fn = parse_macro_input!(item as ItemFn);

    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.push("..");
    buf.push(path);
    let base = buf.as_path();

    let glob = GlobBuilder::new("*.graphql.ron")
        .case_insensitive(true)
        .literal_separator(true)
        .build()
        .unwrap()
        .compile_matcher();

    let walker = WalkDir::new(base);
    let mut test_functions = vec![];
    let mut glob_found_matches = false;

    for file in walker {
        let file = file.unwrap();
        let path = file.path();
        let stripped_path = path.strip_prefix(base).unwrap();
        if !glob.is_match(stripped_path) {
            continue;
        }

        glob_found_matches = true;

        let stem = path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .split_once('.')
            .unwrap()
            .0;
        assert!(!stem.is_empty());
        assert!(stem.chars().all(|x| x.is_alphanumeric() || x == '_'));

        let fn_ident = item_fn.sig.ident.clone();
        let base_ident = Ident::new("base", Span::call_site());
        let base_ident_value = base.to_str().unwrap();
        let stem_ident = Ident::new("stem", Span::call_site());
        let test_function_body = quote! {
            let #base_ident = ::std::path::PathBuf::from(#base_ident_value);
            let #stem_ident = #stem;
            #fn_ident(&#base_ident, #stem_ident);
        };

        let test_function_name = Ident::new(format!("test_{}", stem).as_ref(), Span::call_site());
        let test_fn = quote! {
            #[test]
            fn #test_function_name() {
                #test_function_body
            }
        };

        test_functions.push(proc_macro::TokenStream::from(test_fn));
    }

    assert!(glob_found_matches);

    result.extend(test_functions);
    result
}

#[proc_macro]
pub fn parameterized_file_tests(item: TokenStream) -> TokenStream {
    let input: Ident = syn::parse(item).unwrap();

    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.push("../trustfall_core/src/resources/test_data");
    let base = buf.as_path();

    let glob = GlobBuilder::new("*.graphql.ron")
        .case_insensitive(true)
        .literal_separator(true)
        .build()
        .unwrap()
        .compile_matcher();

    let walker = WalkDir::new(base);
    let mut test_functions = vec![];
    let mut glob_found_matches = false;

    for file in walker {
        let file = file.unwrap();
        let path = file.path();
        let stripped_path = path.strip_prefix(base).unwrap();
        if !glob.is_match(stripped_path) {
            continue;
        }

        glob_found_matches = true;

        let stem = path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .split_once('.')
            .unwrap()
            .0;
        assert!(!stem.is_empty());
        assert!(stem.chars().all(|x| x.is_alphanumeric() || x == '_'));

        let cloned_input = input.clone();
        let base_ident = Ident::new("base", Span::call_site());
        let base_ident_value = base.to_str().unwrap();
        let stem_ident = Ident::new("stem", Span::call_site());
        let test_function_body = quote! {
            let #base_ident = ::std::path::PathBuf::from(#base_ident_value);
            let #stem_ident = #stem;
            #cloned_input(&#base_ident, #stem_ident);
        };

        let test_function_name = Ident::new(format!("test_{}", stem).as_ref(), Span::call_site());
        let test_fn = quote! {
            #[test]
            fn #test_function_name() {
                #test_function_body
            }
        };

        test_functions.push(proc_macro::TokenStream::from(test_fn));
    }

    assert!(glob_found_matches);

    let mut result = TokenStream::new();
    result.extend(test_functions);
    result
}
