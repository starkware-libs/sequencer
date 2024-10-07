use starknet_mempool_infra::component_server::{create_empty_server, WrapperServer};

use crate::http_server::HttpServer as HttpServerComponent;

pub type HttpServer = WrapperServer<HttpServerComponent>;

pub fn create_http_server(http_server: HttpServerComponent) -> HttpServer {
    create_empty_server(http_server)
}
