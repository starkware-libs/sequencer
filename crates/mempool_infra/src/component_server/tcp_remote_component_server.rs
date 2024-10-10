use std::marker::PhantomData;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::error;

use super::definitions::ComponentServerStarter;
use crate::component_definitions::{ComponentRequestHandler, ServerError};

pub struct TCPRemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: DeserializeOwned + Send + 'static,
    Response: Serialize + 'static,
{
    socket: SocketAddr,
    component: Arc<Mutex<Component>>,
    _req: PhantomData<Request>,
    _res: PhantomData<Response>,
}

impl<Component, Request, Response> TCPRemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: DeserializeOwned + Send + 'static,
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
}

#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for TCPRemoteComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + 'static,
    Request: DeserializeOwned + Send + Sync + 'static,
    Response: Serialize + Send + Sync + 'static,
{
    async fn start(&mut self) {
        let listener = TcpListener::bind(self.socket)
            .await
            .unwrap_or_else(|_| panic!("Server should start listening on socket: {}", self.socket));

        while let Ok((mut socket, address)) = listener.accept().await {
            let component = self.component.clone();

            tokio::spawn(async move {
                // Set enough space for the serialized version of Request by adding the usize's size
                // for cases where Request's size < Serialized Request's size. Otherwise read_buf
                // will fail.
                let mut buffer =
                    Vec::<u8>::with_capacity(size_of::<Request>() + size_of::<usize>());

                let response = match socket.read_buf(&mut buffer).await {
                    Ok(n_bytes) => match deserialize(&buffer[..n_bytes]) {
                        Ok(request) => Ok(component.lock().await.handle_request(request).await),
                        Err(error) => {
                            Err(ServerError::RequestDeserializationFailure(error.to_string()))
                        }
                    },
                    Err(error) => Err(ServerError::RequestReadFailure(error.to_string())),
                };

                let response_bytes =
                    serialize(&response).expect("Response serialization should succeed");
                if socket.write_all(&response_bytes).await.is_err() {
                    error!("could not write to remote socket address: {address}");
                }
            });
        }
    }
}
