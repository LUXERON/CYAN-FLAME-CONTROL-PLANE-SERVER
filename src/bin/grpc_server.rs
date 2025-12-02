//! CYAN FLAME™ gRPC Control Plane Server Binary
//!
//! Standalone gRPC server for the CYAN FLAME Virtual GPU Network.
//! Provides calibration matrix distribution, telemetry collection,
//! memory allocation, and agent operations.

use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use symmetrix_core::grpc::{GrpcServerConfig, server::CyanFlameGrpcServer};

/// CYAN FLAME gRPC Control Plane Server
#[derive(Parser, Debug)]
#[command(name = "cyan-flame-grpc-server")]
#[command(author = "SYMMETRIX CORE")]
#[command(version)]
#[command(about = "CYAN FLAME™ gRPC Control Plane Server - Virtual GPU Network")]
struct Args {
    /// Server bind address
    #[arg(short, long, default_value = "0.0.0.0:50051")]
    bind: String,

    /// Enable TLS
    #[arg(long)]
    tls: bool,

    /// TLS certificate path
    #[arg(long)]
    cert: Option<String>,

    /// TLS key path
    #[arg(long)]
    key: Option<String>,

    /// Maximum concurrent streams per connection
    #[arg(long, default_value = "100")]
    max_streams: u32,

    /// Enable gRPC reflection (for debugging)
    #[arg(long)]
    reflection: bool,

    /// Log level (trace, debug, info, warn, error)
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
║                    gRPC Control Plane Server v{}                       ║
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
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    print_banner();

    // Build configuration
    let config = if args.tls {
        let cert = args.cert.ok_or("TLS certificate path required when --tls is enabled")?;
        let key = args.key.ok_or("TLS key path required when --tls is enabled")?;
        let mut config = GrpcServerConfig::production(cert, key);
        config.bind_addr = args.bind;
        config.max_concurrent_streams = args.max_streams;
        config.enable_reflection = args.reflection;
        config
    } else {
        GrpcServerConfig {
            bind_addr: args.bind,
            enable_tls: false,
            cert_path: None,
            key_path: None,
            ca_cert_path: None,
            enable_mtls: false,
            max_concurrent_streams: args.max_streams,
            enable_reflection: args.reflection,
        }
    };

    // Create and start server
    let server = CyanFlameGrpcServer::with_config(config);
    server.serve().await?;

    Ok(())
}

