//! CLI for Starknet OS Runner.

#[cfg(not(feature = "stwo_proving"))]
fn main() {
    eprintln!("The `starknet_os_runner` binary requires the `stwo_proving` feature.");
    std::process::exit(1);
}

use std::net::SocketAddr;

use anyhow::Context;
use clap::Parser;
use jsonrpsee::server::{
    serve_with_graceful_shutdown,
    stop_channel,
    Methods,
    ServerBuilder,
    ServerConfig,
};
use starknet_os_runner::server::config::{CliArgs, ServiceConfig};
use starknet_os_runner::server::cors::{build_cors_layer, cors_mode};
use starknet_os_runner::server::rpc_impl::ProvingRpcServerImpl;
use starknet_os_runner::server::rpc_trait::ProvingRpcServer;
use starknet_os_runner::server::tls::load_tls_acceptor;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tracing::{info, warn};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

#[cfg(feature = "stwo_proving")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::net::SocketAddr;

    use anyhow::Context;
    use clap::Parser;
    use jsonrpsee::server::{ServerBuilder, ServerConfig};
    use starknet_os_runner::server::config::{CliArgs, ServiceConfig};
    use starknet_os_runner::server::cors::{build_cors_layer, cors_mode};
    use starknet_os_runner::server::rpc_impl::ProvingRpcServerImpl;
    use starknet_os_runner::server::rpc_trait::ProvingRpcServer;
    use tower::ServiceBuilder;
    use tracing::info;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    // TODO(Avi): Revisit the starknet_os_runner=debug default once the service stabilizes.
    // Initialize tracing with RUST_LOG (default: info,starknet_os_runner=debug).
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,starknet_os_runner=debug"));
    tracing_subscriber::registry().with(fmt::layer()).with(filter).init();

    // Parse CLI args and load config.
    let args = CliArgs::parse();
    let config = ServiceConfig::from_args(args)?;

    // Build and start the JSON-RPC server.
    let rpc_impl = ProvingRpcServerImpl::from_config(&config);
    let addr = SocketAddr::new(config.ip, config.port);

    let cors_layer = build_cors_layer(&config.cors_allow_origin)?;

    let server_config = ServerConfig::builder().max_connections(config.max_connections).build();
    let server_builder = ServerBuilder::default()
        .set_config(server_config)
        .set_http_middleware(ServiceBuilder::new().option_layer(cors_layer));

    if let (Some(tls_cert_file), Some(tls_key_file)) = (&config.tls_cert_file, &config.tls_key_file)
    {
        let tls_acceptor = load_tls_acceptor(tls_cert_file.as_path(), tls_key_file.as_path())?;
        let listener = TcpListener::bind(addr)
            .await
            .context(format!("Failed to bind HTTPS JSON-RPC server to {addr}"))?;
        let local_addr =
            listener.local_addr().context("Failed to read local address for HTTPS listener")?;

        let methods: Methods = rpc_impl.into_rpc().into();
        let svc_builder = server_builder.to_service_builder();
        let (stop_handle, server_handle) = stop_channel();

        tokio::spawn(async move {
            loop {
                let accept_result = tokio::select! {
                    accept_result = listener.accept() => accept_result,
                    _ = stop_handle.clone().shutdown() => break,
                };

                let (socket, remote_addr) = match accept_result {
                    Ok(conn) => conn,
                    Err(err) => {
                        warn!(error = %err, "Failed to accept incoming TCP connection");
                        continue;
                    }
                };

                let tls_acceptor = tls_acceptor.clone();
                let stop_handle = stop_handle.clone();
                let methods = methods.clone();
                let svc_builder = svc_builder.clone();

                tokio::spawn(async move {
                    let tls_stream = match tls_acceptor.accept(socket).await {
                        Ok(stream) => stream,
                        Err(err) => {
                            warn!(
                                remote_address = %remote_addr,
                                error = %err,
                                "TLS handshake failed"
                            );
                            return;
                        }
                    };

                    let svc = svc_builder.build(methods, stop_handle.clone());
                    if let Err(err) =
                        serve_with_graceful_shutdown(tls_stream, svc, stop_handle.shutdown()).await
                    {
                        warn!(
                            remote_address = %remote_addr,
                            error = %err,
                            "HTTPS connection terminated with error"
                        );
                    }
                });
            }
        });

        info!(
            local_address = %local_addr,
            scheme = "https",
            tls_cert_file = %tls_cert_file.display(),
            tls_key_file = %tls_key_file.display(),
            max_concurrent_requests = config.max_concurrent_requests,
            max_connections = config.max_connections,
            cors_mode = cors_mode(&config.cors_allow_origin),
            cors_allow_origin = ?config.cors_allow_origin,
            "JSON-RPC proving server is running."
        );

        server_handle.stopped().await;
        return Ok(());
    }

    let server = server_builder
        .build(&addr)
        .await
        .context(format!("Failed to bind JSON-RPC server to {addr}"))?;

    let handle = server.start(rpc_impl.into_rpc());
    info!(
        local_address = %addr,
        scheme = "http",
        max_concurrent_requests = config.max_concurrent_requests,
        max_connections = config.max_connections,
        cors_mode = cors_mode(&config.cors_allow_origin),
        cors_allow_origin = ?config.cors_allow_origin,
        "JSON-RPC proving server is running."
    );

    handle.stopped().await;
    Ok(())
}
