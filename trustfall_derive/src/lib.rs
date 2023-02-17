use quote::quote;
use syn::punctuated::Punctuated;

const TRUSTFALL_ATTRIBUTE: &str = "trustfall";
const SKIP_CONVERSION_ATTRIBUTE: &str = "skip_conversion";

/// Adds `typename()` and `as_<variant>()` methods on an enum being used as a Trustfall vertex.
///
/// The `typename()` method is part of Trustfall's [`Typename`] trait.
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
/// will get the following implementations:
/// ```rust
/// # use trustfall_core::interpreter::Typename;
/// #
/// # #[derive(Debug, Clone)]
/// # enum Vertex {
/// #     User(String),
/// #     Message { author: String, content: String },
/// #     EmptyVariant,
/// # }
/// #
/// impl Typename for Vertex {
///     fn typename(&self) -> &'static str {
///         match self {
///             Self::User { .. } => "User",
///             Self::Message { .. } => "Message",
///             Self::EmptyVariant { .. } => "EmptyVariant",
///         }
///     }
/// }
///
/// impl Vertex {
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
///
/// [`Typename`](trustfall_core::interpreter::Typename)
#[proc_macro_derive(TrustfallEnumVertex, attributes(trustfall))]
pub fn trustfall_enum_vertex_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match syn::parse(input) {
        Ok(ast) => impl_trustfall_enum_vertex(&ast).unwrap_or_else(syn::Error::into_compile_error),
        Err(e) => e.into_compile_error(),
    }
    .into()
}

fn impl_trustfall_enum_vertex(ast: &syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

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
        .map(generate_conversion_method)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .reduce(|mut acc, e| {
            acc.extend(e);
            acc
        });

    let conversions_impl = if let Some(conversions) = conversions {
        quote! {
            #[automatically_derived]
            impl #impl_generics #name #ty_generics #where_clause {
                #conversions
            }
        }
    } else {
        Default::default()
    };

    let gen = quote! {
        #[automatically_derived]
        impl #impl_generics ::trustfall_core::interpreter::Typename for #name #ty_generics #where_clause {
            fn typename(&self) -> &'static str {
                match self {
                    #arms

                    #[allow(unreachable_code)]
                    _ => unreachable!("this arm exists only for uninhabited enums"),
                }
            }
        }
        #conversions_impl
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

fn generate_conversion_method(variant: &syn::Variant) -> syn::Result<proc_macro2::TokenStream> {
    // Check if we should skip generating the conversion method
    // because of a `#[trustfall(skip_conversion)]` attribute on the variant.
    for attr in &variant.attrs {
        if let Some(ident) = attr.path.get_ident() {
            if ident != TRUSTFALL_ATTRIBUTE {
                // Not one of our attributes, skip.
                continue;
            }
        }

        // If we ever add more attribute contents, here's how to make the parsing smarter:
        // https://blog.turbo.fish/proc-macro-parsing/
        match attr.parse_meta()? {
            syn::Meta::Path { .. } => {
                return Err(syn::Error::new_spanned(
                    attr,
                    "no arguments found, did you mean `#[trustfall(skip_conversion)]`?",
                ));
            }
            syn::Meta::List(values) => {
                let mut skipping = false;
                for nested in values.nested.iter() {
                    match nested {
                        syn::NestedMeta::Meta(syn::Meta::Path(path)) => {
                            if let Some(ident) = path.get_ident() {
                                if ident == SKIP_CONVERSION_ATTRIBUTE {
                                    skipping = true;
                                } else {
                                    return Err(syn::Error::new_spanned(
                                        nested,
                                        "unexpected arguments found, did you mean `#[trustfall(skip_conversion)]`?",
                                    ));
                                }
                            }
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                attr,
                                "unexpected arguments found, did you mean `#[trustfall(skip_conversion)]`?",
                            ));
                        }
                    }
                }
                if skipping {
                    return Ok(Default::default());
                }
            }
            syn::Meta::NameValue(name_value) => {
                return Err(syn::Error::new_spanned(
                    &name_value.lit,
                    "unexpected arguments found, did you mean `#[trustfall(skip_conversion)]`?",
                ));
            }
        };
    }

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
                Ok(syn::parse_quote! {
                    pub(crate) fn #conversion_name(&self) -> Option<&#field_type> {
                        match self {
                            Self::#variant_ident { #field_name } => Some(#field_name),
                            _ => None,
                        }
                    }
                })
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
                Ok(syn::parse_quote! {
                    pub(crate) fn #conversion_name(&self) -> Option<#final_type> {
                        match self {
                            Self::#variant_ident { #fields } => Some((#fields)),
                            _ => None,
                        }
                    }
                })
            }
        }
        syn::Fields::Unnamed(tuple_fields) if !tuple_fields.unnamed.is_empty() => {
            if tuple_fields.unnamed.len() == 1 {
                // Tuple variants with only a single field return `Option<&ThatField>`.
                let field_type = &tuple_fields.unnamed[0].ty;
                Ok(syn::parse_quote! {
                    pub(crate) fn #conversion_name(&self) -> Option<&#field_type> {
                        match self {
                            Self::#variant_ident(x) => Some(x),
                            _ => None,
                        }
                    }
                })
            } else {
                // Tuple variants with multiple fields return
                // `Option<(&FirstField, &SecondField, ...)>`.
                let final_type: proc_macro2::TokenStream =
                    tuple_of_field_types(&tuple_fields.unnamed);
                let mut fields = syn::punctuated::Punctuated::<_, syn::Token![,]>::new();
                for (i, _) in tuple_fields.unnamed.iter().enumerate() {
                    fields.push(quote::format_ident!("x{i}"));
                }
                Ok(syn::parse_quote! {
                    pub(crate) fn #conversion_name(&self) -> Option<#final_type> {
                        match self {
                            Self::#variant_ident(#fields) => Some((#fields)),
                            _ => None,
                        }
                    }
                })
            }
        }
        _ => {
            // Either unit variant, or fieldless struct/tuple variant.
            Ok(syn::parse_quote! {
                pub(crate) fn #conversion_name(&self) -> Option<()> {
                    match self {
                        Self::#variant_ident => Some(()),
                        _ => None,
                    }
                }
            })
        }
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
