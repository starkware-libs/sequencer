use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident, ItemFn};

/// Constant to control the default time limit for timed tests
const DEFAULT_TIME_LIMIT_MS: u64 = 200;

/// Helper function to parse the time limit from the attribute.
/// Returns the default if the attribute is empty.
fn parse_time_limit(attr: TokenStream, default: u64) -> u64 {
    if attr.is_empty() {
        return default;
    }

    let attr_str = attr.to_string();
    let trimmed = attr_str.trim();

    if trimmed.is_empty() {
        return default;
    }

    trimmed.parse::<u64>().unwrap_or_else(|_| {
        panic!("Expected a positive integer for time limit in milliseconds, got: {}", trimmed)
    })
}

/// Helper to generate sync timing check code
fn sync_timing_check(
    fn_name: &Ident,
    fn_block: &syn::Block,
    time_limit_ms: u64,
) -> proc_macro2::TokenStream {
    quote! {
        let start = std::time::Instant::now();
        #fn_block
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if elapsed_ms > #time_limit_ms {
            panic!(
                "Test `{}` exceeded time limit: took {}ms, limit is {}ms",
                stringify!(#fn_name),
                elapsed_ms,
                #time_limit_ms
            );
        }
    }
}

/// Helper to generate async timing check code with tokio::time::timeout
fn async_timing_check(
    fn_name: &Ident,
    fn_block: &syn::Block,
    time_limit_ms: u64,
) -> proc_macro2::TokenStream {
    quote! {
        let start = tokio::time::Instant::now();
        let result = tokio::time::timeout(tokio::time::Duration::from_millis(#time_limit_ms), async {
            #fn_block
        }).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if result.is_err() || elapsed_ms > #time_limit_ms {
            panic!(
                "Test `{}` exceeded time limit: took {}ms, limit is {}ms",
                stringify!(#fn_name),
                elapsed_ms,
                #time_limit_ms
            );
        }
    }
}

/// Helper to generate the expanded test function with timing checks
fn generate_timed_test(
    attr: TokenStream,
    input: TokenStream,
    test_attrs_before: proc_macro2::TokenStream,
    test_attrs_after: Option<proc_macro2::TokenStream>,
    is_async: bool,
) -> TokenStream {
    let time_limit_ms = parse_time_limit(attr, DEFAULT_TIME_LIMIT_MS);
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_attrs = &input_fn.attrs;
    let fn_vis = &input_fn.vis;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;

    let timing_check = if is_async {
        async_timing_check(fn_name, fn_block, time_limit_ms)
    } else {
        sync_timing_check(fn_name, fn_block, time_limit_ms)
    };

    let test_attrs_after = test_attrs_after.unwrap_or_else(|| quote! {});

    let expanded = quote! {
        #test_attrs_before
        #(#fn_attrs)*
        #test_attrs_after
        #fn_vis #fn_sig {
            #timing_check
        }
    };

    TokenStream::from(expanded)
}

/// A timed version of the `#[test]` attribute that fails if the test exceeds a time limit.
///
/// # Usage
///
/// ```rust,ignore
/// #[timed_test]
/// fn my_test() {
///     // test code
/// }
///
/// // With custom time limit (in milliseconds)
/// #[timed_test(500)]
/// fn my_slower_test() {
///     // test code
/// }
/// ```
///
/// The default time limit is `DEFAULT_TIME_LIMIT_MS` (200ms). If the test exceeds this limit, it
/// will panic with a message indicating how long it took.
#[proc_macro_attribute]
pub fn timed_test(attr: TokenStream, input: TokenStream) -> TokenStream {
    generate_timed_test(attr, input, quote! { #[test] }, None, false)
}

/// A timed version of the `#[tokio::test]` attribute that fails if the test exceeds a time limit.
///
/// # Usage
///
/// ```rust,ignore
/// #[timed_tokio_test]
/// async fn my_test() {
///     // test code
/// }
///
/// // With custom time limit (in milliseconds)
/// #[timed_tokio_test(1000)]
/// async fn my_slower_test() {
///     // test code
/// }
/// ```
///
/// The default time limit is `DEFAULT_TIME_LIMIT_MS` (200ms). If the test exceeds this limit, it
/// will panic with a message indicating how long it took.
#[proc_macro_attribute]
pub fn timed_tokio_test(attr: TokenStream, input: TokenStream) -> TokenStream {
    generate_timed_test(attr, input, quote! { #[tokio::test] }, None, true)
}

/// A timed version of the `#[rstest]` attribute that fails if the test exceeds a time limit.
///
/// # Usage
///
/// ```rust,ignore
/// use apollo_timed_tests::timed_rstest;
///
/// #[timed_rstest]
/// fn my_test(param: u32) {
///     // test code
/// }
///
/// // With custom time limit (in milliseconds)
/// #[timed_rstest(500)]
/// fn my_slower_test(param: u32) {
///     // test code
/// }
/// ```
///
/// The default time limit is `DEFAULT_TIME_LIMIT_MS` (200ms). If the test exceeds this limit, it
/// will panic with a message indicating how long it took.
///
/// For async tests, use `#[timed_rstest_tokio]` instead.
///
/// Note: This macro automatically uses `rstest` internally, so you don't need to import it.
/// You can use `rstest` attributes like `#[case]` and `#[fixture]` by importing them from
/// `apollo_timed_tests`:
/// ```rust,ignore
/// use apollo_timed_tests::{timed_rstest, case, fixture};
/// ```
#[proc_macro_attribute]
pub fn timed_rstest(attr: TokenStream, input: TokenStream) -> TokenStream {
    // Note: This uses apollo_timed_tests::rstest which re-exports rstest, so external crates
    // don't need to add rstest as a direct dependency.
    generate_timed_test(attr, input, quote! { #[apollo_timed_tests::rstest::rstest] }, None, false)
}

/// A timed version of the `#[rstest]` attribute for async tests that fails if the test exceeds a
/// time limit.
///
/// # Usage
///
/// ```rust,ignore
/// use apollo_timed_tests::timed_rstest_tokio;
///
/// #[timed_rstest_tokio]
/// async fn my_test(param: u32) {
///     // test code
/// }
///
/// // With custom time limit (in milliseconds)
/// #[timed_rstest_tokio(500)]
/// async fn my_slower_test(param: u32) {
///     // test code
/// }
/// ```
///
/// The default time limit is `DEFAULT_TIME_LIMIT_MS` (200ms). If the test exceeds this limit, it
/// will panic with a message indicating how long it took.
///
/// Note: This macro automatically uses `rstest` internally, so you don't need to import it.
/// You can use `rstest` attributes like `#[case]` and `#[fixture]` by importing them from
/// `apollo_timed_tests`:
/// ```rust,ignore
/// use apollo_timed_tests::{timed_rstest_tokio, case, fixture};
/// ```
#[proc_macro_attribute]
pub fn timed_rstest_tokio(attr: TokenStream, input: TokenStream) -> TokenStream {
    // Note: This uses apollo_timed_tests::rstest which re-exports rstest, so external crates
    // don't need to add rstest as a direct dependency.
    generate_timed_test(
        attr,
        input,
        quote! { #[apollo_timed_tests::rstest::rstest] },
        Some(quote! { #[tokio::test] }),
        true,
    )
}
