use quote::quote;

#[proc_macro_derive(VariantsAsVertexTypes)]
pub fn variants_as_vertex_types_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse(input).expect("failed to parse input");
    impl_variants_as_vertex_types(&ast)
}

fn impl_variants_as_vertex_types(ast: &syn::DeriveInput) -> proc_macro::TokenStream {
    let name = &ast.ident;
    let generics = &ast.generics;
    let where_clause = &generics.where_clause;

    let variants = match &ast.data {
        syn::Data::Enum(d) => &d.variants,
        _ => panic!("Only enums can become named types!"),
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
                &format!("as_{}", to_snake_case(&variant_name)),
                proc_macro2::Span::call_site(),
            );

            match &variant.fields {
                syn::Fields::Named(named_fields) if !named_fields.named.is_empty() => todo!(),
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
                        let mut punct = syn::punctuated::Punctuated::<_, syn::Token![,]>::new();
                        for field in tuple_fields.unnamed.iter() {
                            let ty = &field.ty;
                            punct.push(quote!(&#ty))
                        }
                        let final_type: proc_macro2::TokenStream = quote!((#punct));

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
                    _ => unreachable!("this arm exists only for uninhabited enums, but was reached anyway: {:?}", self),
                }
            }

            #conversions
        }
    };
    gen.into()
}

fn generate_typename_arm(variant: &syn::Variant) -> proc_macro2::TokenStream {
    let variant_ident = &variant.ident;
    let variant_name = variant_ident.to_string();
    let typename = proc_macro2::Literal::string(&variant_name);
    syn::parse_quote! {
        Self::#variant_ident { .. } => #typename,
    }
}

fn to_snake_case(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for (i, c) in value.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}
