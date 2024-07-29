use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream, Result};
use syn::{parse_macro_input, Ident, Token};

struct MacroInput {
    response_enum: Ident,
    invocation_name: Ident,
    component_client_error: Ident,
    component_error: Ident,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let response_enum = input.parse()?;
        input.parse::<Token![,]>()?;
        let invocation_name = input.parse()?;
        input.parse::<Token![,]>()?;
        let component_client_error = input.parse()?;
        input.parse::<Token![,]>()?;
        let component_error = input.parse()?;
        Ok(MacroInput { response_enum, invocation_name, component_client_error, component_error })
    }
}

/// A macro for generating code that handles the received communication response.
/// Takes the following arguments:
/// * response_enum -- the response enum type
/// * invocation_name -- the request/response enum variant that was invoked
/// * component_client_error -- the component's client error type
/// * component_error --  the component's error type
///
/// For example, the following input:
/// """
/// handle_response_variants!(MempoolResponse, GetTransactions, MempoolClientError, MempoolError)
/// """
///
/// Results in:
/// """
/// match response {
///     MempoolResponse::GetTransactions(Ok(response)) => Ok(response),
///     MempoolResponse::GetTransactions(Err(response)) => {
///         Err(MempoolClientError::MempoolError(response))
///     }
///     unexpected_response => Err(MempoolClientError::ClientError(
///         ClientError::UnexpectedResponse(format!("{unexpected_response:?}")),
///     )),
/// }
/// """
#[proc_macro]
pub fn handle_response_variants(input: TokenStream) -> TokenStream {
    let MacroInput { response_enum, invocation_name, component_client_error, component_error } =
        parse_macro_input!(input as MacroInput);

    let expanded = quote! {
        match response {
            #response_enum::#invocation_name(Ok(response)) => Ok(response),
            #response_enum::#invocation_name(Err(response)) => {
                Err(#component_client_error::#component_error(response))
            }
            unexpected_response => Err(#component_client_error::ClientError(ClientError::UnexpectedResponse(format!("{unexpected_response:?}")))),
        }
    };

    TokenStream::from(expanded)
}
