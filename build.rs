//! Build script for CYAN FLAME Control Plane Server
//! Compiles Protocol Buffer definitions into Rust code using tonic-build
//! Falls back to pre-generated files if protoc is not available

use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the output directory for generated code
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);

    // Tell Cargo to rerun build.rs if proto files change
    println!("cargo:rerun-if-changed=proto/cyan_flame.proto");
    println!("cargo:rerun-if-changed=proto/");

    // Check if protoc is available
    let protoc_available = Command::new("protoc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if protoc_available {
        // protoc is available - generate fresh from .proto
        println!("cargo:warning=Using protoc to generate gRPC code from proto files");

        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
            .file_descriptor_set_path(out_dir.join("cyan_flame_descriptor.bin"))
            .compile_protos(
                &["proto/cyan_flame.proto"],
                &["proto/"],
            )?;
    } else {
        // protoc is NOT available - use pre-generated files
        println!("cargo:warning=protoc not found, using pre-generated gRPC files");

        let pre_generated_rs = PathBuf::from("src/generated/cyan_flame.v1.rs");
        let pre_generated_bin = PathBuf::from("src/generated/cyan_flame_descriptor.bin");

        if !pre_generated_rs.exists() || !pre_generated_bin.exists() {
            return Err("Pre-generated proto files not found. Please install protoc or ensure src/generated/ contains the generated files.".into());
        }

        // Copy pre-generated files to OUT_DIR
        std::fs::copy(&pre_generated_rs, out_dir.join("cyan_flame.v1.rs"))?;
        std::fs::copy(&pre_generated_bin, out_dir.join("cyan_flame_descriptor.bin"))?;
    }

    Ok(())
}

