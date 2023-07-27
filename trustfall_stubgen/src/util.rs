use quote::quote;

pub(crate) fn to_lower_snake_case(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut last = '_';
    for c in value.chars() {
        if c.is_uppercase() {
            if last != '_' && !last.is_uppercase() {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
        last = c;
    }
    result
}

pub(crate) fn upper_case_variant_name(value: &str) -> String {
    let mut chars = value.chars();
    let first_char = chars.next().expect("unexpectedly got an empty string").to_ascii_uppercase();
    let rest_chars: String = chars.collect();

    format!("{}{}", first_char, rest_chars)
}

pub(crate) fn property_resolver_fn_name(type_name: &str) -> String {
    let normalized_name = to_lower_snake_case(type_name);
    format!("resolve_{normalized_name}_property")
}

pub(crate) fn type_edge_resolver_fn_name(type_name: &str) -> String {
    let normalized_name = to_lower_snake_case(type_name);
    format!("resolve_{normalized_name}_edge")
}

pub(crate) fn parse_import(import: &str) -> Vec<String> {
    import.split("::").map(|x| x.to_string()).collect()
}

pub(crate) fn trustfall_type_to_rust_type(trustfall_type: &str) -> proc_macro2::TokenStream {
    let mut processed_type = trustfall_type;

    let mut nullable = true;
    if let Some(inner) = processed_type.strip_suffix('!') {
        processed_type = inner;
        nullable = false;
    }

    let ty = {
        if let Some(partial) = processed_type.strip_prefix('[') {
            let inner = partial
                .strip_suffix(']')
                .unwrap_or_else(|| panic!("invalid Trustfall type started with `[` without matching `]`: {trustfall_type}"));
            let inner_ty = trustfall_type_to_rust_type(inner);

            quote! {
                Vec<#inner_ty>
            }
        } else {
            match processed_type {
                "Int" => quote! { i64 }, // TODO: this is technically incorrect, it might be u64 too
                "String" => quote! { &str },
                "Float" => quote! { f64 },
                "Boolean" => quote! { bool },
                _ => unimplemented!(
                    "type {processed_type} is not yet supported when autogenerating stubs"
                ),
            }
        }
    };

    if nullable {
        quote! {
            Option<#ty>
        }
    } else {
        ty
    }
}

pub(crate) fn field_value_to_rust_type(
    trustfall_type: &str,
    base: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let mut processed_type = trustfall_type;

    let mut nullable = true;
    if let Some(inner) = processed_type.strip_suffix('!') {
        processed_type = inner;
        nullable = false;
    }

    let suffix = if let Some(partial) = processed_type.strip_prefix('[') {
        let inner = partial.strip_suffix(']').unwrap_or_else(|| {
            panic!("invalid Trustfall type started with `[` without matching `]`: {trustfall_type}")
        });
        let base = quote! {
            |value| value
        };
        let inner_tokens = field_value_to_rust_type(inner, base);

        if nullable {
            quote! {
                as_slice()
                    .map(|slice| {
                        slice
                            .iter()
                            .map(#inner_tokens)
                            .collect()
                    })
            }
        } else {
            quote! {
                as_slice()
                    .expect("expected a list-typed value but did not get a list")
                    .iter()
                    .map(#inner_tokens)
                    .collect()
            }
        }
    } else {
        let mut conv_fn = match processed_type {
            "Int" => quote! { as_i64() }, // TODO: this is technically incorrect, it might be u64 too
            "String" => quote! { as_str() },
            "Float" => quote! { as_f64() },
            "Boolean" => quote! { as_bool() },
            _ => unimplemented!(
                "type {processed_type} is not yet supported when autogenerating stubs"
            ),
        };
        if !nullable {
            let expect_msg = format!(
                "unexpected null or other incorrect datatype for Trustfall type '{trustfall_type}'"
            );
            conv_fn = quote! {
                #conv_fn.expect(#expect_msg)
            };
        }

        conv_fn
    };

    quote! {
        #base.#suffix
    }
}

pub fn escaped_rust_name(name: String) -> String {
    // https://doc.rust-lang.org/reference/keywords.html
    match name.as_str() {
        "as" | "break" | "const" | "continue" | "crate" | "else" | "enum" | "extern" | "false"
        | "fn" | "for" | "if" | "impl" | "in" | "let" | "loop" | "match" | "mod" | "move"
        | "mut" | "pub" | "ref" | "return" | "self" | "Self" | "static" | "struct" | "super"
        | "trait" | "true" | "type" | "unsafe" | "use" | "where" | "while" | "async" | "await"
        | "dyn" | "try" | "macro_rules" | "union" | "'static" => name + "_",
        _ => name,
    }
}
