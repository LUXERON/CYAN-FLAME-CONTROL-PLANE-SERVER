//! Build script for CYAN FLAME Control Plane Server
//! Compiles Protocol Buffer definitions into Rust code using tonic-build

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the output directory for generated code
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    
    // Configure tonic-build for server-side code
    tonic_build::configure()
        // Generate server code (Control Plane serves gRPC)
        .build_server(true)
        // Also generate client code (for internal testing)
        .build_client(true)
        // Enable useful derives
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        // Output file descriptor set for gRPC reflection
        .file_descriptor_set_path(out_dir.join("cyan_flame_descriptor.bin"))
        // Compile the proto file
        .compile_protos(
            &["proto/cyan_flame.proto"],
            &["proto/"],
        )?;

    // Tell Cargo to rerun build.rs if proto files change
    println!("cargo:rerun-if-changed=proto/cyan_flame.proto");
    
    Ok(())
}

