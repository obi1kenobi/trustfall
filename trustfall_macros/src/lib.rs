use core::panicking::panic;

use proc_macro::TokenStream;
use quote::quote;
use syn::{self, Data};

#[proc_macro_derive(NamedType)]
pub fn named_type_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_named_type(&ast)
}

fn impl_named_type(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;

    todo!("Iterate over variants, adding the string form of their name for each");

    let variants = match &ast.data {
        Data::Enum(d) => d.variants,
        _ => panic!("Only enums can become named types!"),
    };

    let gen = quote! {
        impl NamedType for #name {
            fn typename(&self) -> &'static str {
                match self {
                    _ => unreachable!(),
                }
            }
        }
    };
    gen.into()
}
