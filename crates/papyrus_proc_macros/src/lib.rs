use std::str::FromStr;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, ExprLit, Ident, ItemFn, ItemTrait, LitBool, LitStr, Meta, TraitItem};

/// This macro is a wrapper around the "rpc" macro supplied by the jsonrpsee library that generates
/// a server and client traits from a given trait definition. The wrapper gets a version id and
/// prepend the version id to the trait name and to every method name (note method name refers to
/// the name the API has for the function not the actual function name). We need this in order to be
/// able to merge multiple versions of jsonrpc APIs into one server and not have a clash in method
/// resolution.
///
/// # Example:
///
/// Given this code:
/// ```rust,ignore
/// #[versioned_rpc("V0_6_0")]
/// pub trait JsonRpc {
///     #[method(name = "blockNumber")]
///     fn block_number(&self) -> Result<BlockNumber, Error>;
/// }
/// ```
///
/// The macro will generate this code:
/// ```rust,ignore
/// #[rpc(server, client, namespace = "starknet")]
/// pub trait JsonRpcV0_6_0 {
///     #[method(name = "V0_6_0_blockNumber")]
///     fn block_number(&self) -> Result<BlockNumber, Error>;
/// }
/// ```
#[proc_macro_attribute]
pub fn versioned_rpc(attr: TokenStream, input: TokenStream) -> TokenStream {
    let version = parse_macro_input!(attr as syn::LitStr);
    let item_trait = parse_macro_input!(input as ItemTrait);

    let trait_name = &item_trait.ident;
    let visibility = &item_trait.vis;

    // generate the new method signatures with the version prefix
    let versioned_methods = item_trait
        .items
        .iter()
        .map(|item| {
            if let TraitItem::Fn(method) = item {
                let new_method = syn::TraitItemFn {
                    attrs: method
                        .attrs
                        .iter()
                        .filter(|attr| !matches!(attr.meta, Meta::NameValue(_)))
                        .map(|attr| {
                            let mut new_attr = attr.clone();
                            if attr.path().is_ident("method") {
                                let _ = attr.parse_nested_meta(|meta| {
                                    if meta.path.is_ident("name") {
                                        let value = meta.value()?;
                                        let method_name: LitStr = value.parse()?;
                                        let new_meta_str = format!(
                                            "method(name = \"{}_{}\")",
                                            version.value(),
                                            method_name.value()
                                        );
                                        new_attr.meta = syn::parse_str::<Meta>(&new_meta_str)?;
                                    }
                                    Ok(())
                                });
                            }
                            new_attr
                        })
                        .collect::<Vec<_>>(),
                    sig: method.sig.clone(),
                    default: method.default.clone(),
                    semi_token: method.semi_token,
                };
                new_method.into()
            } else {
                item.clone()
            }
        })
        .collect::<Vec<TraitItem>>();

    // generate the versioned trait with the new method signatures
    let versioned_trait = syn::ItemTrait {
        attrs: vec![syn::parse_quote!(#[rpc(server, client, namespace = "starknet")])],
        vis: visibility.clone(),
        unsafety: None,
        auto_token: None,
        ident: syn::Ident::new(&format!("{}{}", trait_name, version.value()), trait_name.span()),
        colon_token: None,
        supertraits: item_trait.supertraits.clone(),
        brace_token: item_trait.brace_token,
        items: versioned_methods,
        restriction: item_trait.restriction.clone(),
        generics: item_trait.generics.clone(),
        trait_token: item_trait.trait_token,
    };

    versioned_trait.to_token_stream().into()
}

/// This macro will emit a histogram metric with the given name and the latency of the function.
/// In addition, also a debug log with the metric name and the execution time will be emitted.
/// The macro also receives a boolean for whether it will be emitted only when
/// profiling is activated or at all times.
///
/// # Example
/// Given this code:
///
/// ```rust,ignore
/// #[latency_histogram("metric_name", false)]
/// fn foo() {
///     // Some code ...
/// }
/// ```
/// Every call to foo will update the histogram metric with the name “metric_name” with the time it
/// took to execute foo. In addition, a debug log with the following format will be emitted:
/// “<metric_name>: <execution_time>”
/// The metric will be emitted regardless of the value of the profiling configuration,
/// since the config value is false.
#[proc_macro_attribute]
pub fn latency_histogram(attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut input_fn = parse_macro_input!(input as ItemFn);
    let parts = attr
        .to_string()
        .split(',')
        .map(|s| {
            TokenStream::from_str(s)
                .expect("Expecting metric name and bool (is for profiling only)")
        })
        .collect::<Vec<_>>();
    let metric_name_as_tokenstream = parts
        .first()
        .expect("attribute should include metric name and controll with config boolean")
        .clone();
    // TODO(DanB): consider naming the input value instead of providing a bool
    // TODO(DanB): consider adding support for metrics levels (e.g. debug, info, warn, error)
    // instead of boolean
    let controll_with_config_as_tokenstream = parts
        .get(1)
        .expect("attribute should include metric name and controll with config boolean")
        .clone();
    let metric_name = parse_macro_input!(metric_name_as_tokenstream as ExprLit);
    let controll_with_config = parse_macro_input!(controll_with_config_as_tokenstream as LitBool);
    let origin_block = &mut input_fn.block;

    // Create a new block with the metric update.
    let expanded_block = quote! {
        {
            let mut start_function_time = None;
            if !#controll_with_config || (#controll_with_config && *(papyrus_common::metrics::COLLECT_PROFILING_METRICS.get().unwrap_or(&false))) {
                start_function_time=Some(std::time::Instant::now());
            }
            let return_value=#origin_block;
            if let Some(start_time) = start_function_time {
                let exec_time = start_time.elapsed().as_secs_f64();
                metrics::histogram!(#metric_name).record(exec_time);
                tracing::debug!("{}: {}", #metric_name, exec_time);
            }
            return_value
        }
    };

    // Create a new function with the modified block.
    let modified_function = ItemFn {
        block: syn::parse2(expanded_block).expect("Parse tokens in latency_histogram attribute."),
        ..input_fn
    };

    modified_function.to_token_stream().into()
}

struct HandleAllResponseVariantsMacroInput {
    response_enum: Ident,
    request_response_enum_var: Ident,
    component_client_error: Ident,
    component_error: Ident,
    response_type: Ident,
}

impl syn::parse::Parse for HandleAllResponseVariantsMacroInput {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let response_enum: Ident = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let request_response_enum_var: Ident = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let component_client_error: Ident = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let component_error: Ident = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let response_type: Ident = input.parse()?;

        Ok(HandleAllResponseVariantsMacroInput {
            response_enum,
            request_response_enum_var,
            component_client_error,
            component_error,
            response_type,
        })
    }
}

/// A macro for generating code that sends the request and handles the received response.
/// Takes the following arguments:
/// * response_enum -- the response enum type
/// * request_response_enum_var -- the request/response enum variant corresponding to the invoked
///   function
/// * component_client_error -- the component client error type
/// * component_error --  the component error type
/// * response_type -- Boxed or Direct, a string literal indicating if the response content is boxed
///   or not
///
/// For example, use of the Direct response_type:
/// ```rust,ignore
/// handle_all_response_variants!(MempoolResponse, GetTransactions, MempoolClientError, MempoolError, Direct)
/// ```
///
/// Results in:
/// ```rust,ignore
/// let response = self.send(request).await;
/// match response? {
///     MempoolResponse::GetTransactions(Ok(resp)) => Ok(resp),
///     MempoolResponse::GetTransactions(Err(resp)) => {
///         Err(MempoolClientError::MempoolError(resp))
///     }
///     unexpected_response => Err(MempoolClientError::ClientError(
///         ClientError::UnexpectedResponse(format!("{unexpected_response:?}")),
///     )),
/// }
/// ```
/// Use of the Boxed response_type:
/// ```rust,ignore
/// handle_all_response_variants!(MempoolResponse, GetTransactions, MempoolClientError, MempoolError, Boxed)
/// ```
///
/// Results in:
/// ```rust,ignore
/// let response = self.send(request).await;
/// match response? {
///     MempoolResponse::GetTransactions(Ok(boxed_resp)) => {
///         let resp = *boxed_resp;
///         Ok(resp)
///     }
///     MempoolResponse::GetTransactions(Err(resp)) => {
///         Err(MempoolClientError::MempoolError(resp))
///     }
///     unexpected_response => Err(MempoolClientError::ClientError(
///         ClientError::UnexpectedResponse(format!("{unexpected_response:?}")),
///     )),
/// }
/// ```
#[proc_macro]
pub fn handle_all_response_variants(input: TokenStream) -> TokenStream {
    let HandleAllResponseVariantsMacroInput {
        response_enum,
        request_response_enum_var,
        component_client_error,
        component_error,
        response_type,
    } = parse_macro_input!(input as HandleAllResponseVariantsMacroInput);

    let mut expanded = match response_type.to_string().as_str() {
        "Boxed" => quote! {
            {
                // Dereference the Box to get the response value
                let resp = *resp;
                Ok(resp)
            }
        },
        "Direct" => quote! {
            Ok(resp),
        },
        _ => panic!("Expected 'Boxed' or 'Direct'"),
    };

    expanded = quote! {
        {
            let response = self.send(request).await;
            match response? {
                #response_enum::#request_response_enum_var(Ok(resp)) =>
                    #expanded
                #response_enum::#request_response_enum_var(Err(resp)) => {
                    Err(#component_client_error::#component_error(resp))
                }
                unexpected_response => Err(#component_client_error::ClientError(ClientError::UnexpectedResponse(format!("{unexpected_response:?}")))),
            }
        }
    };

    TokenStream::from(expanded)
}
