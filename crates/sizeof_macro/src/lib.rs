// The code in this file uses code from and inspired by https://github.com/dtolnay/syn/tree/master/examples/heapsize and https://github.com/Kixiron/size-of

extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input,
    parse_quote,
    Data,
    DataEnum,
    DataStruct,
    DeriveInput,
    Fields,
    GenericParam,
    Generics,
};

/// This macro derives the `SizeOf` trait for structs and enums, allowing them to calculate
/// their dynamic size and total size in bytes.
///
/// Use `size_bytes()` to get the total size of the type, which includes both stack and heap parts.
/// Use `dynamic_size()` to get the heap size of the type.
///
/// `WARNING`: It is `DANGEROUS` to use this macro on types with children that implement `Deref`,
/// for two reasons (the first leading to under-counting and the second to over-counting):
/// 1. If a type `P<T>` does not explicitly implement `SizeOf`, calling `P<T>.size_bytes()` will
///    implicitly call `T.size_bytes()` (due to `Deref coercion`), which will neglect counting the
///    stack size taken by `P<T>` itself. Currently, this macro is only implemented for the
///    following `Deref` types: `Box<T>, Rc<T>, Arc<T>, RefCell<T>, Mutex<T>, RwLock<T>`. If your
///    `Deref` type is not on this list, refrain from using this macro.
/// 2. Even if a pointer type `P<T>` implements `SizeOf` (for example `Rc<T>` or `Arc<T>`), it is
///    possible to count the same memory location twice if multiple pointers point to the same data.
///    In such cases it is recommended to implement `SizeOf` manually for the type.
///
/// Example usage for structs:
/// ```rust
/// #[derive(SizeOf)]
/// struct MyStruct {
///     a: u32,
///     b: String,
/// }
/// ```
/// This will implement the following for `MyStruct`:
/// ```rust
/// impl SizeOf for MyStruct {
///     fn dynamic_size(&self) -> usize {
///         let mut size = 0;
///         size += self.a.dynamic_size();
///         size += self.b.dynamic_size();
///         size
///     }
/// }
/// ```
///
/// Example usage for enums:
/// ```rust
/// #[derive(SizeOf)]
/// enum MyEnum {
///     VariantA(u32),
///     VariantB { x: u64, y: String },
/// }
/// ```
/// This will implement the following for `MyEnum`:
/// ```rust
/// impl SizeOf for MyEnum {
///     fn dynamic_size(&self) -> usize {
///         match self {
///             MyEnum::VariantA(value) => {
///                 let mut size = 0;
///                 size += value.dynamic_size();
///                 size
///             }
///             MyEnum::VariantB { x, y } => {
///                 let mut size = 0;
///                 size += x.dynamic_size();
///                 size += y.dynamic_size();
///                 size
///             }
///         }
///     }
/// }
/// ```
#[proc_macro_derive(SizeOf)]
pub fn derive_dynamic_size(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident; // The name of the struct or enum being derived.
    let generics = add_trait_bounds(input.generics); // Add the SizeOf trait bound
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl(); // Split the generics into parts for the impl block.

    let gen = match input.data {
        Data::Struct(data_struct) => derive_macro_for_struct(data_struct),
        Data::Enum(data_enum) => derive_macro_for_enum(data_enum),
        Data::Union(_) => unimplemented!("SizeOf can only be derived for structs and enums."),
    };

    // Create the implementation block for the SizeOf trait.
    // The `impl_generics`, `ty_generics`, and `where_clause` are used to ensure the T: SizeOf bound
    // is enforced on all generic types.
    quote! {
        impl #impl_generics SizeOf for #name #ty_generics #where_clause {
            fn dynamic_size(&self) -> usize {
                #gen
            }
        }
    }
    .into()
}

// Helper function to derive dynamic_size() for structs.
fn derive_macro_for_struct(data_struct: DataStruct) -> TokenStream2 {
    // Generate expressions for each field in the struct to calculate their sizes.
    let field_exprs = match data_struct.fields {
        Fields::Named(ref fields) => fields
            .named
            .iter()
            .map(|f| {
                let ident = f.ident.as_ref().unwrap(); // Get the name of the field.
                quote_spanned! {f.span()=>
                    size += self.#ident.dynamic_size();
                }
            })
            .collect::<Vec<_>>(),
        Fields::Unnamed(ref fields) => fields
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let idx = syn::Index::from(i); // Create an index for unnamed fields.
                quote_spanned! {f.span()=>
                    size += self.#idx.dynamic_size();
                }
            })
            .collect(),
        Fields::Unit => vec![],
    };
    quote! {
       let mut size = 0;
       // Calculate the size of each field in the struct.
        #(#field_exprs)*
         size
    }
}

// Helper functions to derive dynamic_size() for enums.
fn derive_macro_for_enum(data_enum: DataEnum) -> TokenStream2 {
    // Generate match arms for each variant of the enum.
    let variant_matches = data_enum.variants.iter().map(|variant| {
        let vname = &variant.ident;

        // Match arms for each variant, calculating the size based on its fields.
        match &variant.fields {
            Fields::Named(ref fields) => {
                let idents: Vec<_> =
                    fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                let bindings: Vec<_> = idents.iter().map(|id| quote! { #id }).collect();
                let sizes: Vec<_> = idents
                    .iter()
                    .map(|id| quote_spanned! { id.span() => size += #id.dynamic_size(); })
                    .collect();
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
                let sizes: Vec<_> = bindings
                    .iter()
                    .map(|b| quote_spanned! { b.span() => size += #b.dynamic_size(); })
                    .collect();
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
        // Match on the enum variants and calculate their dynamic sizes.
        match self {
            // Each variant match arm will calculate its dynamic size.
            #(#variant_matches),*
        }
    }
}

// Add a bound `T: SizeOf` to every type parameter T.
fn add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param.bounds.push(parse_quote!(sizeof::SizeOf));
        }
    }
    generics
}
