use quote::quote;
use syn::punctuated::Punctuated;

/// Adds `typename()` and `as_<variant>()` methods on an enum being used as a Trustfall vertex.
///
/// For example:
/// ```rust
/// # use trustfall_derive::TrustfallEnumVertex;
/// #
/// #[derive(Debug, Clone, TrustfallEnumVertex)]
/// enum Vertex {
///     User(String),
///     Message { author: String, content: String },
///     EmptyVariant,
/// }
/// ```
/// will get the following methods:
/// ```rust
/// # #[derive(Debug, Clone)]
/// # enum Vertex {
/// #     User(String),
/// #     Message { author: String, content: String },
/// #     EmptyVariant,
/// # }
/// #
/// impl Vertex {
///     fn typename(&self) -> &'static str {
///         match self {
///             Self::User { .. } => "User",
///             Self::Message { .. } => "Message",
///             Self::EmptyVariant { .. } => "EmptyVariant",
///         }
///     }
///
///     fn as_user(&self) -> Option<&String> {
///         match self {
///             Self::User(x) => Some(x),
///             _ => None,
///         }
///     }
///
///     fn as_message(&self) -> Option<(&String, &String)> {
///         match self {
///             Self::Message { author, content } => Some((author, content)),
///             _ => None,
///         }
///     }
///
///     fn as_empty_variant(&self) -> Option<()> {
///         match self {
///             Self::EmptyVariant => Some(()),
///             _ => None,
///         }
///     }
/// }
/// ```
#[proc_macro_derive(TrustfallEnumVertex)]
pub fn trustfall_enum_vertex_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match syn::parse(input) {
        Ok(ast) => impl_trustfall_enum_vertex(&ast).unwrap_or_else(syn::Error::into_compile_error),
        Err(e) => e.into_compile_error(),
    }
    .into()
}

fn impl_trustfall_enum_vertex(ast: &syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &ast.ident;
    let generics = &ast.generics;
    let where_clause = &generics.where_clause;

    let variants = match &ast.data {
        syn::Data::Enum(d) => &d.variants,
        _ => {
            return Err(syn::Error::new_spanned(
                ast,
                "only enums can derive TrustfallEnumVertex",
            ))
        }
    };

    let arms = variants
        .iter()
        .map(generate_typename_arm)
        .reduce(|mut acc, e| {
            acc.extend(e);
            acc
        })
        .unwrap_or_default();

    let conversions = variants
        .iter()
        .map(|variant| -> proc_macro2::TokenStream {
            let variant_ident = &variant.ident;
            let variant_name = variant_ident.to_string();
            let conversion_name = syn::Ident::new(
                &format!("as_{}", to_lower_snake_case(&variant_name)),
                proc_macro2::Span::call_site(),
            );

            match &variant.fields {
                syn::Fields::Named(named_fields) if !named_fields.named.is_empty() => {
                    if named_fields.named.len() == 1 {
                        // Struct variants with only a single field return `Option<&ThatField>`.
                        let named_arg = &named_fields.named[0];
                        let field_name = named_arg
                            .ident
                            .as_ref()
                            .expect("struct variant field had no name");
                        let field_type = &named_arg.ty;
                        syn::parse_quote! {
                            pub(crate) fn #conversion_name(&self) -> Option<&#field_type> {
                                match self {
                                    Self::#variant_ident { #field_name } => Some(#field_name),
                                    _ => None,
                                }
                            }
                        }
                    } else {
                        // Struct variants with multiple fields return
                        // `Option<(&FirstField, &SecondField, ...)>`
                        // in the order the fields were defined.
                        let final_type: proc_macro2::TokenStream =
                            tuple_of_field_types(&named_fields.named);

                        let mut fields = syn::punctuated::Punctuated::<_, syn::Token![,]>::new();
                        for field in named_fields.named.iter() {
                            let field_name = field
                                .ident
                                .as_ref()
                                .expect("struct variant field had no name");
                            fields.push(field_name);
                        }
                        syn::parse_quote! {
                            pub(crate) fn #conversion_name(&self) -> Option<#final_type> {
                                match self {
                                    Self::#variant_ident { #fields } => Some((#fields)),
                                    _ => None,
                                }
                            }
                        }
                    }
                }
                syn::Fields::Unnamed(tuple_fields) if !tuple_fields.unnamed.is_empty() => {
                    if tuple_fields.unnamed.len() == 1 {
                        // Tuple variants with only a single field return `Option<&ThatField>`.
                        let field_type = &tuple_fields.unnamed[0].ty;
                        syn::parse_quote! {
                            pub(crate) fn #conversion_name(&self) -> Option<&#field_type> {
                                match self {
                                    Self::#variant_ident(x) => Some(x),
                                    _ => None,
                                }
                            }
                        }
                    } else {
                        // Tuple variants with multiple fields return
                        // `Option<(&FirstField, &SecondField, ...)>`.
                        let final_type: proc_macro2::TokenStream =
                            tuple_of_field_types(&tuple_fields.unnamed);
                        let mut fields = syn::punctuated::Punctuated::<_, syn::Token![,]>::new();
                        for (i, _) in tuple_fields.unnamed.iter().enumerate() {
                            fields.push(quote::format_ident!("x{i}"));
                        }
                        syn::parse_quote! {
                            pub(crate) fn #conversion_name(&self) -> Option<#final_type> {
                                match self {
                                    Self::#variant_ident(#fields) => Some((#fields)),
                                    _ => None,
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Either unit variant, or fieldless struct/tuple variant.
                    syn::parse_quote! {
                        pub(crate) fn #conversion_name(&self) -> Option<()> {
                            match self {
                                Self::#variant_ident => Some(()),
                                _ => None,
                            }
                        }
                    }
                }
            }
        })
        .reduce(|mut acc, e| {
            acc.extend(e);
            acc
        })
        .unwrap_or_default();

    let gen = quote! {
        #[automatically_derived]
        impl #generics #name #generics #where_clause {
            pub(crate) fn typename(&self) -> &'static str {
                match self {
                    #arms

                    #[allow(unreachable_code)]
                    _ => unreachable!("this arm exists only for uninhabited enums"),
                }
            }

            #conversions
        }
    };
    Ok(gen)
}

fn generate_typename_arm(variant: &syn::Variant) -> proc_macro2::TokenStream {
    let variant_ident = &variant.ident;
    let variant_name = variant_ident.to_string();
    let typename = proc_macro2::Literal::string(&variant_name);
    syn::parse_quote! {
        Self::#variant_ident { .. } => #typename,
    }
}

/// Returns a tuple of references to all field types.
/// The input must contain more than one field.
fn tuple_of_field_types(
    fields: &Punctuated<syn::Field, syn::Token![,]>,
) -> proc_macro2::TokenStream {
    if fields.len() > 1 {
        let mut punct = syn::punctuated::Punctuated::<_, syn::Token![,]>::new();
        for field in fields.iter() {
            let ty = &field.ty;
            punct.push(quote!(&#ty))
        }
        quote!((#punct))
    } else {
        panic!(
            "list of fields had {} field(s), which is not more than one field",
            fields.len()
        );
    }
}

fn to_lower_snake_case(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut last = '_';
    for c in value.chars() {
        if c.is_uppercase() {
            if last != '_' {
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
