use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server, StatusCode};
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use validator::Validate;

use super::definitions::ComponentServerStarter;
use crate::component_definitions::{
    ComponentRequestHandler,
    ServerError,
    APPLICATION_OCTET_STREAM,
};

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RemoteComponentServerConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub retries: u32,
}

const DEFAULT_RETRIES: u32 = 3;

impl SerializeConfig for RemoteComponentServerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ip",
                &self.ip.to_string(),
                "The remote component server ip.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "port",
                &self.port,
                "The remote component server port.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "retries",
                &self.retries,
                "The max number of retries for sending a message.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for RemoteComponentServerConfig {
    fn default() -> Self {
        Self { ip: "0.0.0.0".parse().unwrap(), port: 8080, retries: DEFAULT_RETRIES }
    }
}

pub struct RemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: for<'a> Deserialize<'a> + Send + 'static,
    Response: Serialize + 'static,
{
    socket: SocketAddr,
    component: Arc<Mutex<Component>>,
    _req: PhantomData<Request>,
    _res: PhantomData<Response>,
}

impl<Component, Request, Response> RemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: for<'a> Deserialize<'a> + Send + 'static,
    Response: Serialize + 'static,
{
    pub fn new(component: Component, ip_address: IpAddr, port: u16) -> Self {
        Self {
            component: Arc::new(Mutex::new(component)),
            socket: SocketAddr::new(ip_address, port),
            _req: PhantomData,
            _res: PhantomData,
        }
    }

    async fn handler(
        http_request: HyperRequest<Body>,
        component: Arc<Mutex<Component>>,
    ) -> Result<HyperResponse<Body>, hyper::Error> {
        let body_bytes = to_bytes(http_request.into_body()).await?;
        let http_response = match deserialize(&body_bytes) {
            Ok(component_request) => {
                // Acquire the lock for component computation, release afterwards.
                let component_response =
                    { component.lock().await.handle_request(component_request).await };
                HyperResponse::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
                    .body(Body::from(
                        serialize(&component_response)
                            .expect("Response serialization should succeed"),
                    ))
            }
            Err(error) => {
                let server_error = ServerError::RequestDeserializationFailure(error.to_string());
                HyperResponse::builder().status(StatusCode::BAD_REQUEST).body(Body::from(
                    serialize(&server_error).expect("Server error serialization should succeed"),
                ))
            }
        }
        .expect("Response building should succeed");

        Ok(http_response)
    }
}

#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for RemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: for<'a> Deserialize<'a> + Send + Sync + 'static,
    Response: Serialize + Send + Sync + 'static,
{
    async fn start(&mut self) {
        let make_svc = make_service_fn(|_conn| {
            let component = Arc::clone(&self.component);
            async {
                Ok::<_, hyper::Error>(service_fn(move |req| {
                    Self::handler(req, Arc::clone(&component))
                }))
            }
        });

        Server::bind(&self.socket.clone()).serve(make_svc).await.unwrap();
    }
}
