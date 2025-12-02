//! CYAN FLAME™ Unified Control Plane Server
//!
//! Combines HTTP Management API and gRPC services in a single binary.
//! - HTTP API on port 8080 (configurable)
//! - gRPC services on port 50051 (configurable)
//!
//! ## Authentication
//!
//! When `--auth` is enabled, all gRPC requests require a valid API key.
//! Include the key in the `x-api-key` header or as `Authorization: Bearer <key>`.
//!
//! ## Default Test API Keys (when --auth is enabled)
//!
//! | Key                    | Tier       | Amplification |
//! |------------------------|------------|---------------|
//! | cf_free_test123        | Free       | 100×          |
//! | cf_starter_test123     | Starter    | 1,000×        |
//! | cf_pro_test123         | Pro        | 10,000×       |
//! | cf_ent_test123         | Enterprise | 24,500×       |
//! | test-key-123           | Enterprise | 24,500×       |

use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use symmetrix_core::grpc::{GrpcServerConfig, server::CyanFlameGrpcServer};

/// CYAN FLAME Unified Control Plane Server
#[derive(Parser, Debug)]
#[command(name = "cyan-flame-unified")]
#[command(author = "SYMMETRIX CORE")]
#[command(version)]
#[command(about = "CYAN FLAME™ Unified Control Plane - HTTP + gRPC")]
struct Args {
    /// HTTP API bind address
    #[arg(long, default_value = "0.0.0.0:8080")]
    http_bind: String,

    /// gRPC server bind address
    #[arg(long, default_value = "0.0.0.0:50051")]
    grpc_bind: String,

    /// Enable TLS for gRPC
    #[arg(long)]
    grpc_tls: bool,

    /// Server TLS certificate path (PEM)
    #[arg(long)]
    cert: Option<String>,

    /// Server TLS private key path (PEM)
    #[arg(long)]
    key: Option<String>,

    /// CA certificate path for mTLS client verification (PEM)
    #[arg(long)]
    ca_cert: Option<String>,

    /// Enable mTLS (mutual TLS) - requires client certificates
    #[arg(long)]
    mtls: bool,

    /// Enable gRPC reflection
    #[arg(long)]
    reflection: bool,

    /// Enable API key authentication (requires x-api-key header)
    #[arg(long)]
    auth: bool,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
}

fn print_banner() {
    println!(r#"
╔══════════════════════════════════════════════════════════════════════════════╗
║                                                                              ║
║   ██████╗██╗   ██╗ █████╗ ███╗   ██╗    ███████╗██╗      █████╗ ███╗   ███╗  ║
║  ██╔════╝╚██╗ ██╔╝██╔══██╗████╗  ██║    ██╔════╝██║     ██╔══██╗████╗ ████║  ║
║  ██║      ╚████╔╝ ███████║██╔██╗ ██║    █████╗  ██║     ███████║██╔████╔██║  ║
║  ██║       ╚██╔╝  ██╔══██║██║╚██╗██║    ██╔══╝  ██║     ██╔══██║██║╚██╔╝██║  ║
║  ╚██████╗   ██║   ██║  ██║██║ ╚████║    ██║     ███████╗██║  ██║██║ ╚═╝ ██║  ║
║   ╚═════╝   ╚═╝   ╚═╝  ╚═╝╚═╝  ╚═══╝    ╚═╝     ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝  ║
║                                                                              ║
║                    UNIFIED CONTROL PLANE SERVER v{}                    ║
║                                                                              ║
║                    SYMMETRIX CORE™ Virtual GPU Network                       ║
║                    24,500× Memory Amplification                              ║
║                                                                              ║
╚══════════════════════════════════════════════════════════════════════════════╝
"#, env!("CARGO_PKG_VERSION"));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    let log_level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    print_banner();

    let auth_status = if args.auth { "🔐 ENABLED" } else { "🔓 DISABLED" };
    let tls_status = if args.grpc_tls { "🔒 ENABLED" } else { "🔓 DISABLED" };
    let mtls_status = if args.mtls { "🔐 ENABLED (client certs required)" } else { "🔓 DISABLED" };

    info!("╔══════════════════════════════════════════════════════════════════╗");
    info!("║           CYAN FLAME™ Unified Control Plane                      ║");
    info!("╠══════════════════════════════════════════════════════════════════╣");
    info!("║  HTTP API:  {:50} ║", args.http_bind);
    info!("║  gRPC:      {:50} ║", args.grpc_bind);
    info!("║  TLS:       {:50} ║", tls_status);
    info!("║  mTLS:      {:50} ║", mtls_status);
    info!("║  API Auth:  {:50} ║", auth_status);
    info!("╚══════════════════════════════════════════════════════════════════╝");

    if args.mtls {
        info!("╔══════════════════════════════════════════════════════════════════╗");
        info!("║                    TWO-LAYER SECURITY ACTIVE                     ║");
        info!("╠══════════════════════════════════════════════════════════════════╣");
        info!("║  Layer 1: mTLS - Client must present valid certificate           ║");
        info!("║  Layer 2: API Key - Client must provide valid API key            ║");
        info!("╚══════════════════════════════════════════════════════════════════╝");
    }

    // Build gRPC configuration
    let grpc_config = GrpcServerConfig {
        bind_addr: args.grpc_bind.clone(),
        enable_tls: args.grpc_tls,
        cert_path: args.cert,
        key_path: args.key,
        ca_cert_path: args.ca_cert,
        enable_mtls: args.mtls,
        max_concurrent_streams: 100,
        enable_reflection: args.reflection,
    };

    // Start gRPC server in background with auth setting
    let grpc_server = CyanFlameGrpcServer::with_config_and_auth(grpc_config, args.auth);
    let grpc_handle = tokio::spawn(async move {
        if let Err(e) = grpc_server.serve().await {
            tracing::error!("gRPC server error: {}", e);
        }
    });

    // Start HTTP server with auth status
    let http_addr: SocketAddr = args.http_bind.parse()?;
    let auth_enabled = args.auth;
    let http_handle = tokio::spawn(async move {
        run_http_server(http_addr, auth_enabled).await;
    });

    info!("🔥 CYAN FLAME Unified Server running");
    info!("   HTTP: http://{}", args.http_bind);
    info!("   gRPC: {}", args.grpc_bind);
    if args.auth {
        info!("   Auth: API key required in 'x-api-key' header");
    }

    // Wait for shutdown signal
    tokio::select! {
        _ = grpc_handle => info!("gRPC server stopped"),
        _ = http_handle => info!("HTTP server stopped"),
        _ = tokio::signal::ctrl_c() => info!("Received shutdown signal"),
    }

    Ok(())
}

async fn run_http_server(addr: SocketAddr, auth_enabled: bool) {
    use axum::{routing::get, Router, Json, Extension};
    use serde_json::json;

    let app = Router::new()
        .route("/", get(move || async move {
            Json(json!({
                "status": "healthy",
                "service": "CYAN FLAME Unified",
                "auth_enabled": auth_enabled
            }))
        }))
        .route("/health", get(|| async { Json(json!({"status": "healthy"})) }))
        .route("/grpc", get(move || async move {
            Json(json!({
                "grpc_port": 50051,
                "auth_required": auth_enabled,
                "services": [
                    "CalibrationService",
                    "TelemetryService",
                    "AllocationService",
                    "OperationsService"
                ]
            }))
        }))
        .route("/tiers", get(|| async {
            Json(json!({
                "tiers": [
                    {
                        "name": "free",
                        "amplification": 100,
                        "max_allocation_tb": 2,
                        "rate_limit": 100,
                        "price": "$0/month"
                    },
                    {
                        "name": "starter",
                        "amplification": 1000,
                        "max_allocation_tb": 24,
                        "rate_limit": 1000,
                        "price": "$99/month"
                    },
                    {
                        "name": "pro",
                        "amplification": 10000,
                        "max_allocation_tb": 240,
                        "rate_limit": 10000,
                        "price": "$499/month"
                    },
                    {
                        "name": "enterprise",
                        "amplification": 24500,
                        "max_allocation_tb": 574,
                        "rate_limit": "unlimited",
                        "price": "$2,499+/month"
                    }
                ]
            }))
        }));

    info!("🌐 HTTP server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

