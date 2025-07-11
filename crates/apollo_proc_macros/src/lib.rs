use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use lazy_static::lazy_static;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    parse,
    parse2,
    parse_macro_input,
    parse_str,
    Expr,
    ExprLit,
    Ident,
    ItemFn,
    ItemTrait,
    LitBool,
    LitStr,
    Meta,
    Token,
    TraitItem,
};

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
    let (metric_name, control_with_config, input_fn) = parse_latency_histogram_attributes::<ExprLit>(
        attr,
        input,
        "Expecting a string literal for metric name",
    );

    // TODO(DanB): consider naming the input value instead of providing a bool
    // TODO(DanB): consider adding support for metrics levels (e.g. debug, info, warn, error)
    // instead of boolean

    let metric_recording_logic = quote! {
        ::metrics::histogram!(#metric_name).record(exec_time);
    };

    let collect_metric_flag = quote! {
        papyrus_common::metrics::COLLECT_PROFILING_METRICS
    };

    create_modified_function(
        control_with_config,
        input_fn,
        metric_recording_logic,
        collect_metric_flag,
    )
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
/// use apollo_metrics::metrics::{MetricHistogram, MetricScope};
///
/// const FOO_HISTOGRAM_METRIC: MetricHistogram = MetricHistogram::new(
///     MetricScope::Infra,
///     "foo_histogram_metric",
///     "foo function latency histogram metrics",
/// );
///
/// #[sequencer_latency_histogram(FOO_HISTOGRAM_METRIC, false)]
/// fn foo() {
///     // Some code ...
/// }
/// ```
/// Every call to foo will update the histogram metric FOO_HISTOGRAM_METRIC with the time it
/// took to execute foo. In addition, a debug log with the following format will be emitted:
/// “<metric_name>: <execution_time>”
/// The metric will be emitted regardless of the value of the profiling configuration,
/// since the config value is false.
#[proc_macro_attribute]
pub fn sequencer_latency_histogram(attr: TokenStream, input: TokenStream) -> TokenStream {
    let (metric_name, control_with_config, input_fn) = parse_latency_histogram_attributes::<Ident>(
        attr,
        input,
        "Expecting an identifier for metric name",
    );

    let metric_recording_logic = quote! {
        #metric_name.record(exec_time);
    };

    let collect_metric_flag = quote! {
        apollo_metrics::metrics::COLLECT_SEQUENCER_PROFILING_METRICS
    };

    create_modified_function(
        control_with_config,
        input_fn,
        metric_recording_logic,
        collect_metric_flag,
    )
}

/// Helper function to parse the attributes and input for the latency histogram macros.
fn parse_latency_histogram_attributes<T: Parse>(
    attr: TokenStream,
    input: TokenStream,
    err_msg: &str,
) -> (T, LitBool, ItemFn) {
    let binding = attr.to_string();
    let parts: Vec<&str> = binding.split(',').collect();
    let metric_name_string = parts
        .first()
        .expect("attribute should include metric name and control with config boolean")
        .trim()
        .to_string();
    let control_with_config_string = parts
        .get(1)
        .expect("attribute should include metric name and control with config boolean")
        .trim()
        .to_string();

    let control_with_config = parse_str::<LitBool>(&control_with_config_string)
        .expect("Expecting a boolean value for control with config");
    let metric_name = parse_str::<T>(&metric_name_string).expect(err_msg);

    let input_fn = parse::<ItemFn>(input).expect("Failed to parse input as ItemFn");

    (metric_name, control_with_config, input_fn)
}

/// Helper function to create the expanded block and modified function.
fn create_modified_function(
    control_with_config: LitBool,
    input_fn: ItemFn,
    metric_recording_logic: impl ToTokens,
    collect_metric_flag: impl ToTokens,
) -> TokenStream {
    // Create a new block with the metric update.
    let origin_block = &input_fn.block;
    let expanded_block = quote! {
        {
            let mut start_function_time = None;
            if !#control_with_config || (#control_with_config && *(#collect_metric_flag.get().unwrap_or(&false))) {
                start_function_time = Some(std::time::Instant::now());
            }
            let return_value = #origin_block;
            if let Some(start_time) = start_function_time {
                let exec_time = start_time.elapsed().as_secs_f64();
                #metric_recording_logic
            }
            return_value
        }
    };

    // Create a new function with the modified block.
    let modified_function = ItemFn {
        block: parse2(expanded_block).expect("Parse tokens in latency_histogram attribute."),
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

impl Parse for HandleAllResponseVariantsMacroInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let response_enum: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let request_response_enum_var: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let component_client_error: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let component_error: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
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

fn get_uniq_identifier_for_call_site(identifier_prefix: &str) -> Ident {
    // Use call site span for uniqueness
    let span = proc_macro::Span::call_site();
    let span_str = format!("{span:?}");

    let mut hasher = DefaultHasher::new();
    span_str.hash(&mut hasher);

    let hash_id = format!("{:x}", hasher.finish()); // short identifier
    let ident_str = format!("__{identifier_prefix}_{hash_id}");
    Ident::new(&ident_str, proc_macro2::Span::call_site())
}

struct LogEveryNMacroInput {
    log_macro: syn::Path,
    n: Expr,
    args: Punctuated<Expr, Token![,]>,
}

impl Parse for LogEveryNMacroInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let log_macro: syn::Path = input.parse()?;
        input.parse::<Token![,]>()?;
        let n: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let args: Punctuated<Expr, Token![,]> = Punctuated::parse_terminated(input)?;

        Ok(LogEveryNMacroInput { log_macro, n, args })
    }
}

/// An internal helper macro for logging a message every `n` calls to the macro.
/// Do not use this directly. Instead use the `info_every_n!`, `debug_every_n!`, etc. macros.
#[proc_macro]
pub fn log_every_n(input: TokenStream) -> TokenStream {
    let LogEveryNMacroInput { log_macro, n, args, .. } =
        parse_macro_input!(input as LogEveryNMacroInput);

    // Use call site span for uniqueness
    let span = proc_macro::Span::call_site();
    let span_str = format!("{span:?}");

    let mut hasher = DefaultHasher::new();
    span_str.hash(&mut hasher);

    let hash_id = format!("{:x}", hasher.finish()); // short identifier
    let ident_str = format!("__TRACING_COUNT_{hash_id}");
    let ident = Ident::new(&ident_str, proc_macro2::Span::call_site());

    let args = args.into_iter().collect::<Vec<_>>();

    let expanded = quote! {
        {
            static #ident: ::std::sync::OnceLock<::std::sync::atomic::AtomicUsize> = ::std::sync::OnceLock::new();
            let counter = #ident.get_or_init(|| ::std::sync::atomic::AtomicUsize::new(0));
            let current_count = counter.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed);

            if current_count % (#n) == 0 {
                #log_macro!(#(#args),*);
            }
        }
    };

    TokenStream::from(expanded)
}

struct LogEveryNSecMacroInput {
    log_macro: syn::Path,
    n: Expr,
    args: Punctuated<Expr, Token![,]>,
}

impl Parse for LogEveryNSecMacroInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let log_macro: syn::Path = input.parse()?;
        input.parse::<Token![,]>()?;
        let n: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let args: Punctuated<Expr, Token![,]> = Punctuated::parse_terminated(input)?;

        Ok(LogEveryNSecMacroInput { log_macro, n, args })
    }
}

lazy_static! {
    static ref LOG_EVERY_N_SEC_CLOCK_START: Instant = Instant::now();
}

/// An internal helper macro for logging a message at most once every `n` seconds.
/// Do not use this directly. Instead use the `info_every_n_sec!`, `debug_every_n_sec!`, etc.
/// macros.
#[proc_macro]
pub fn log_every_n_sec(input: TokenStream) -> TokenStream {
    let LogEveryNSecMacroInput { log_macro, n, args, .. } =
        parse_macro_input!(input as LogEveryNSecMacroInput);

    let ident_last_log_time = get_uniq_identifier_for_call_site("TRACING_LAST_LOG_TIME");
    let ident_start_time = get_uniq_identifier_for_call_site("TRACING_START_TIME");

    let args = args.into_iter().collect::<Vec<_>>();

    let expanded = quote! {
        {
            // We use this to measure the passage of time. We don't use the system time since
            // it can go backwards (e.g. when the system clock is updated).
            static #ident_start_time: ::std::sync::OnceLock<::std::time::Instant> = ::std::sync::OnceLock::new();

            static #ident_last_log_time: ::std::sync::OnceLock<::std::sync::atomic::AtomicU64> = ::std::sync::OnceLock::new();
            let last_log_u64 = #ident_last_log_time.get_or_init(|| ::std::sync::atomic::AtomicU64::new(0));

            match last_log_u64.fetch_update(
                ::std::sync::atomic::Ordering::Relaxed,
                ::std::sync::atomic::Ordering::Relaxed,
                |curr_val : u64| {
                    // We use millis and not secs to avoid having any rounding issues (e.g. 1.9
                    // seconds).
                    let now_with_zero : u64 = #ident_start_time.get_or_init(|| ::std::time::Instant::now())
                        .elapsed().as_millis().try_into()
                        .expect("Timestamp in millis is larger than u64::MAX");
                    // We add +1 to avoid having a value of 0 which can be confused with the first
                    // call.
                    let now : u64 = now_with_zero + 1;

                    if curr_val == 0 {
                        // First call, update the time to start counting from now.
                        return Some(now);
                    }
                    if curr_val + (#n * 1000) <= now {
                        // We should log. Next log should be after n seconds from now.
                        return Some(now);
                    }
                    None
                }
            ) {
                Ok(old_now) => {
                    // We updated the last log time, meaning we should log.
                    #log_macro!(#(#args),*);
                }
                Err(_) => {
                    // We should not log.
                }
            };
        }
    };

    TokenStream::from(expanded)
}
