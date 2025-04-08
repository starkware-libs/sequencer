use hyper::{Body, Request};
use jsonrpsee::core::http_helpers::read_body;
use regex::Regex;
use tower::BoxError;
use tracing::debug;

use crate::version_config::{VERSION_0_8, VERSION_PATTERN};
use crate::SERVER_MAX_BODY_SIZE;

/// [`Tower`] middleware intended to proxy method requests to the last supported version of the API,
/// which is V0_8. The middleware reads the JsonRPC request body and request path then prefixes the
/// method name with the appropriate version identifier. It returns a new [`hyper::Request`] object
/// with the new method name.
///
/// # Arguments
/// * req - [`hyper::Request`] object passed by the server.
///
/// [`Tower`]: https://crates.io/crates/tower
pub(crate) async fn proxy_rpc_request(req: Request<Body>) -> Result<Request<Body>, BoxError> {
    debug!("proxy_rpc_request -> Request received: {:?}", req);

    // Sanity check.
    if !is_supported_path(req.uri().path()) {
        return Err(BoxError::from("Unsupported path for request"));
    }

    let prefix = VERSION_0_8.name;
    let (parts, body) = req.into_parts();
    let (body_bytes, is_single) =
        read_body(&parts.headers, body, SERVER_MAX_BODY_SIZE).await.map_err(BoxError::from)?;
    let new_body = match is_single {
        true => {
            let body = serde_json::from_slice::<jsonrpsee::types::Request<'_>>(&body_bytes)?;
            add_version_to_method_name_in_body(vec![body], prefix, is_single)
        }
        false => {
            let vec_body =
                serde_json::from_slice::<Vec<jsonrpsee::types::Request<'_>>>(&body_bytes)?;
            add_version_to_method_name_in_body(vec_body, prefix, is_single)
        }
    }?;
    Ok(Request::from_parts(parts, new_body.into()))
}

fn add_version_to_method_name_in_body(
    mut vec_body: Vec<jsonrpsee::types::Request<'_>>,
    prefix: &str,
    is_single: bool,
) -> Result<Vec<u8>, BoxError> {
    let Ok(vec_body) = vec_body
        .iter_mut()
        .map(|body| {
            let Some(stripped_method) = strip_starknet_from_method(body.method.as_ref()) else {
                return Err(BoxError::from("Method name has unexpected format"));
            };
            body.method = format!("starknet_{prefix}_{stripped_method}").into();
            Ok(body)
        })
        .collect::<Result<Vec<_>, _>>()
    else {
        return Err(BoxError::from("Method name has unexpected format"));
    };
    let serialized = match is_single {
        true => serde_json::to_vec(&vec_body[0]),
        false => serde_json::to_vec(&vec_body),
    };
    serialized.map_err(BoxError::from)
}

/// this assumes that all methods are of the form:
/// starknet_OnlyOneUnderScoreAndMethodNameIsCamleCased
fn strip_starknet_from_method(method: &str) -> Option<&str> {
    let split_method_name = method.split('_').collect::<Vec<_>>();
    split_method_name.get(1).copied()
}

fn is_supported_path(path: &str) -> bool {
    let re = Regex::new((r"^\/rpc(\/".to_string() + VERSION_PATTERN + ")?$").as_str())
        .expect("should be a valid regex");
    re.is_match(path)
}
