//! JSON-RPC HTTP server for the proving service.
//!
//! This module provides the JSON-RPC server using jsonrpsee, following the
//! pattern established in apollo_rpc.

use std::net::SocketAddr;

use jsonrpsee::server::ServerBuilder;
use tracing::info;

use super::config::ServiceConfig;
use super::rpc_impl::ProvingRpcServerImpl;
use super::rpc_trait::ProvingRpcServer;

/// The JSON-RPC proving server.
pub struct ProvingRpcHttpServer {
    config: ServiceConfig,
    rpc_impl: ProvingRpcServerImpl,
}

impl ProvingRpcHttpServer {
    /// Creates a new ProvingRpcHttpServer.
    pub fn new(config: ServiceConfig) -> Self {
        let rpc_impl = ProvingRpcServerImpl::from_config(&config);
        Self { config, rpc_impl }
    }

    /// Runs the JSON-RPC server.
    ///
    /// This method blocks until the server is stopped.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = SocketAddr::new(self.config.ip, self.config.port);

        let server = ServerBuilder::default().build(&addr).await?;

        let methods = self.rpc_impl.clone().into_rpc();
        let handle = server.start(methods);

        info!(local_address = %addr, "JSON-RPC proving server is running.");
        handle.stopped().await;
        Ok(())
    }
}
