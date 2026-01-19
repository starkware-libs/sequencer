/// A macro for generating code that sends the request and handles the received response.
/// Takes the following arguments:
/// * self -- the self reference to the component client
/// * request -- the request to send
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
/// handle_all_response_variants!(self, request, MempoolResponse, GetTransactions, MempoolClientError, MempoolError, Direct)
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
/// handle_all_response_variants!(self, request, MempoolResponse, GetTransactions, MempoolClientError, MempoolError, Boxed)
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
#[macro_export]
macro_rules! handle_all_response_variants {
    // Pattern for Direct response type
    (
        $self:expr,
        $request:expr,
        $response_enum:ident,
        $request_response_enum_var:ident,
        $component_client_error:ident,
        $component_error:ident,Direct
    ) => {{
        let response = $self.send($request).await;
        match response? {
            $response_enum::$request_response_enum_var(Ok(resp)) => Ok(resp),
            $response_enum::$request_response_enum_var(Err(resp)) => {
                Err($component_client_error::$component_error(resp))
            }
            unexpected_response => Err($component_client_error::ClientError(
                $crate::component_client::ClientError::UnexpectedResponse(format!(
                    "{unexpected_response:?}"
                )),
            )),
        }
    }};
    // Pattern for Boxed response type
    (
        $self:expr,
        $request:expr,
        $response_enum:ident,
        $request_response_enum_var:ident,
        $component_client_error:ident,
        $component_error:ident,Boxed
    ) => {{
        let response = $self.send($request).await;
        match response? {
            $response_enum::$request_response_enum_var(Ok(boxed_resp)) => {
                // Dereference the Box to get the response value
                let resp = *boxed_resp;
                Ok(resp)
            }
            $response_enum::$request_response_enum_var(Err(resp)) => {
                Err($component_client_error::$component_error(resp))
            }
            unexpected_response => Err($component_client_error::ClientError(
                $crate::component_client::ClientError::UnexpectedResponse(format!(
                    "{unexpected_response:?}"
                )),
            )),
        }
    }};
}
