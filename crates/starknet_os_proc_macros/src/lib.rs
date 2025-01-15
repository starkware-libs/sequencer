// TODO(Dori): Uncomment this next line when these macros are actually used.
// extern crate proc_macro;

use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type};

#[proc_macro_derive(CairoType)]
pub fn cairo_type_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    // Get the identifier of the struct
    let struct_ident = &input.ident;

    // Generate code to implement the trait
    let expanded = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => {
                let field_names_read = fields.named.iter().map(|f| &f.ident);
                let field_names_write = field_names_read.clone();
                let n_fields = field_names_read.clone().count();
                let field_values =
                    field_names_read.clone().enumerate().map(|(index, field_name)| {
                        quote! {
                            let #field_name = vm.get_integer((address + #index)?)?.into_owned();
                        }
                    });

                quote! {
                    impl CairoType for #struct_ident {
                        fn from_memory(vm: &VirtualMachine, address: Relocatable) -> Result<Self, MemoryError> {
                         #(#field_values)*
                            Ok(Self {
                                #( #field_names_read ),*
                            })
                        }
                        fn to_memory(&self, vm: &mut VirtualMachine, address: Relocatable) -> Result<(), MemoryError> {
                            let mut offset = 0;
                            #(vm.insert_value((address + offset)?, &self.#field_names_write)?; offset += 1;)*

                            Ok(())
                        }

                        fn n_fields() -> usize {
                            #n_fields
                        }
                    }
                }
            }
            Fields::Unnamed(_) | Fields::Unit => {
                // Unsupported field types
                quote! {
                    compile_error!("CairoType only supports structs with named fields");
                }
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            // Unsupported data types
            quote! {
                compile_error!("CairoType only supports structs");
            }
        }
    };

    // Convert the generated code into a TokenStream and return it
    proc_macro::TokenStream::from(expanded)
}

fn field_size(field: &syn::Field) -> proc_macro2::TokenStream {
    if let Type::Path(type_path) = &field.ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = &segment.ident;
            if type_name == "Felt252" || type_name == "Relocatable" {
                quote! { 1 }
            } else {
                quote! { #type_name::cairo_size() }
            }
        } else {
            let field_name = field.ident.as_ref().unwrap();
            quote! {
                compile_error!("Could not determine the size of {}.", #field_name);
            }
        }
    } else {
        quote! {
            compile_error!("Could not determine the size of all fields in the struct. This derive macro is only compatible with Felt252 fields.");
        }
    }
}

/// Provides a method to compute the address of each field
#[proc_macro_derive(FieldOffsetGetters)]
pub fn get_field_offsets_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    // Get the identifier of the struct
    let struct_ident = &input.ident;

    // Generate code to implement the trait
    let getters = match &input.data {
        Data::Struct(data_struct) => {
            // Extract fields' names and types
            let fields = match &data_struct.fields {
                Fields::Named(fields) => &fields.named,
                _ => {
                    return quote! {
                        compile_error!("FieldOffsetGetters only supports structs with named fields");
                    }
                    .into();
                }
            };

            let mut field_sizes: Vec<proc_macro2::TokenStream> = vec![];
            let mut get_field_offset_methods: Vec<proc_macro2::TokenStream> = vec![];

            for field in fields {
                let field_name = field.ident.as_ref().expect("Expected named field");
                let offset_fn_name = format_ident!("{}_offset", field_name);

                let rendered_offset_impl = if field_sizes.is_empty() {
                    quote! { 0 }
                } else {
                    quote! { #(#field_sizes)+* }
                };

                let get_offset_method = quote! {
                    pub fn #offset_fn_name() -> usize {
                        #rendered_offset_impl
                    }
                };
                get_field_offset_methods.push(get_offset_method);
                field_sizes.push(field_size(field));
            }

            // Combine all setter methods
            quote! {
                impl #struct_ident {
                    #( #get_field_offset_methods )*
                    pub fn cairo_size() -> usize {
                        #(#field_sizes)+*
                    }
                }
            }
        }
        _ => {
            quote! {
                compile_error!("FieldOffsetGetters only supports structs");
            }
        }
    };

    // Convert the generated code into a TokenStream and return it
    getters.into()
}
