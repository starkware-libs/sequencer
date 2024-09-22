use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::component_definitions::ComponentRequestHandler;
use crate::component_runner::ComponentStarter;

pub trait ComponentBounds<Request, Response>:
    ComponentRequestHandler<Request, Response> + Send + Sync + ComponentStarter + 'static
{
}
pub trait ResponseBounds: Serialize + Send + Sync + 'static {}
pub trait RequestBounds: DeserializeOwned + Send + Sync + 'static {}

impl<T, Request, Response> ComponentBounds<Request, Response> for T where
    T: ComponentRequestHandler<Request, Response> + Send + Sync + ComponentStarter + 'static
{
}
impl<T> RequestBounds for T where T: DeserializeOwned + Send + Sync + 'static {}
impl<T> ResponseBounds for T where T: Serialize + Send + Sync + 'static {}
