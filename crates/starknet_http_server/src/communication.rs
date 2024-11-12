use starknet_sequencer_infra::component_server::WrapperServer;

use crate::http_server::HttpServer as HttpServerComponent;

pub type HttpServer = WrapperServer<HttpServerComponent>;
