extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// This macro derives the `SizeOf` trait for structs and enums, allowing them to calculate
/// their size in bytes. The `SizeOf` trait requires implementing the `size_bytes`
/// method, which returns the size of the type in bytes.
#[proc_macro_derive(SizeOf)]
pub fn derive_size_of(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let gen = match input.data {
        Data::Struct(data_struct) => {
            let field_exprs = match data_struct.fields {
                Fields::Named(ref fields) => fields
                    .named
                    .iter()
                    .map(|f| {
                        let ident = f.ident.as_ref().unwrap();
                        quote! {
                            size += self.#ident.size_bytes();
                        }
                    })
                    .collect::<Vec<_>>(),
                Fields::Unnamed(ref fields) => (0..fields.unnamed.len())
                    .map(|i| {
                        let idx = syn::Index::from(i);
                        quote! {
                            size += self.#idx.size_bytes();
                        }
                    })
                    .collect(),
                Fields::Unit => vec![],
            };
            quote! {
                impl SizeOf for #name {
                    fn size_bytes(&self) -> usize {
                        let mut size = 0;
                        #(#field_exprs)*
                        size
                    }
                }
            }
        }
        Data::Enum(data_enum) => {
            let variant_matches = data_enum.variants.iter().map(|variant| {
                let vname = &variant.ident;
                match &variant.fields {
                    Fields::Named(ref fields) => {
                        let idents: Vec<_> =
                            fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                        let bindings: Vec<_> = idents.iter().map(|id| quote! { #id }).collect();
                        let sizes: Vec<_> =
                            idents.iter().map(|id| quote! { size += #id.size_bytes(); }).collect();
                        quote! {
                            Self::#vname { #(#bindings),* } => {
                                let mut size = 0;
                                #(#sizes)*
                                size
                            }
                        }
                    }
                    Fields::Unnamed(ref fields) => {
                        let bindings: Vec<syn::Ident> = (0..fields.unnamed.len())
                            .map(|i| syn::Ident::new(&format!("f{}", i), vname.span()))
                            .collect();
                        let sizes: Vec<_> =
                            bindings.iter().map(|b| quote! { size += #b.size_bytes(); }).collect();
                        quote! {
                            Self::#vname( #(#bindings),* ) => {
                                let mut size = 0;
                                #(#sizes)*
                                size
                            }
                        }
                    }
                    Fields::Unit => {
                        quote! {
                            Self::#vname => 0
                        }
                    }
                }
            });
            quote! {
                impl SizeOf for #name {
                    fn size_bytes(&self) -> usize {
                        match self {
                            #(#variant_matches),*
                        }
                    }
                }
            }
        }
        _ => unimplemented!("SizeOf can only be derived for structs and enums."),
    };
    gen.into()
}
