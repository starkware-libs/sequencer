use std::marker::PhantomData;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use bincode::{deserialize, serialize};
use hyper::body::to_bytes;
use hyper::header::CONTENT_TYPE;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request as HyperRequest, Response as HyperResponse, Server};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;

use crate::component_definitions::{
    ComponentRequestAndResponseSender, ComponentRequestHandler, APPLICATION_OCTET_STREAM,
};
use crate::component_runner::ComponentRunner;

pub struct ComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send + Sync,
    Response: Send + Sync,
{
    component: Component,
    rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
}

impl<Component, Request, Response> ComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response>,
    Request: Send + Sync,
    Response: Send + Sync,
{
    pub fn new(
        component: Component,
        rx: Receiver<ComponentRequestAndResponseSender<Request, Response>>,
    ) -> Self {
        Self { component, rx }
    }
}

#[async_trait]
pub trait ComponentServerStarter: Send + Sync {
    async fn start(&mut self);
}

#[async_trait]
impl<Component, Request, Response> ComponentServerStarter
    for ComponentServer<Component, Request, Response>
where
    Component: ComponentRequestHandler<Request, Response> + Send + Sync,
    Request: Send + Sync,
    Response: Send + Sync,
{
    async fn start(&mut self) {
        while let Some(request_and_res_tx) = self.rx.recv().await {
            let request = request_and_res_tx.request;
            let tx = request_and_res_tx.tx;

            let res = self.component.handle_request(request).await;

            tx.send(res).await.expect("Response connection should be open.");
        }
    }
}

pub struct ComponentServerHttp<Component, Request, Response>
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

impl<Component, Request, Response> ComponentServerHttp<Component, Request, Response>
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

    pub async fn start(&mut self) {
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

    async fn handler(
        http_request: HyperRequest<Body>,
        component: Arc<Mutex<Component>>,
    ) -> Result<HyperResponse<Body>, hyper::Error> {
        let body_bytes = to_bytes(http_request.into_body()).await?;
        let component_request: Request =
            deserialize(&body_bytes).expect("Request deserialization should succeed");

        // Acquire the lock for component computation, release afterwards.
        let component_response;
        {
            let mut component_guard = component.lock().await;
            component_response = component_guard.handle_request(component_request).await;
        }
        let http_response = HyperResponse::builder()
            .header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
            .body(Body::from(
                serialize(&component_response).expect("Response serialization should succeed"),
            ))
            .expect("Response builidng should succeed");

        Ok(http_response)
    }
}

pub struct EmptyServer<T: ComponentRunner + Send + Sync> {
    component: T,
}

impl<T: ComponentRunner + Send + Sync> EmptyServer<T> {
    pub fn new(component: T) -> Self {
        Self { component }
    }
}

#[async_trait]
impl<T: ComponentRunner + Send + Sync> ComponentServerStarter for EmptyServer<T> {
    async fn start(&mut self) {
        match self.component.start().await {
            Ok(_) => println!("ComponentServer::start() completed."),
            Err(err) => println!("ComponentServer::start() failed: {:?}", err),
        }
    }
}

pub fn create_empty_server<T: ComponentRunner + Send + Sync>(component: T) -> EmptyServer<T> {
    EmptyServer::new(component)
}
