#![forbid(unsafe_code)]

use std::{env, path::PathBuf};

fn main() {
    let firmware_crate_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not set CARGO_MANIFEST_DIR"),
    );
    let workspace_dir = env::var_os("CARGO_WORKSPACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| firmware_crate_dir.join("../.."));
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_dir.join("target"));
    let build_inputs_dir = target_dir.join("prototyper");

    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not set OUT_DIR"));
    let config = build_inputs_dir.join("generated_config.rs");
    std::fs::copy(&config, output_dir.join("generated_config.rs"))
        .expect("copy generated firmware configuration");
    println!("cargo:rerun-if-changed={}", config.display());

    for (feature, file_name) in [
        ("CARGO_FEATURE_PAYLOAD", "payload.o"),
        ("CARGO_FEATURE_FDT", "fdt.o"),
    ] {
        if env::var_os(feature).is_some() {
            let object = build_inputs_dir.join(file_name);
            assert!(
                object.is_file(),
                "missing embedded-image object '{}'; run `cargo prototyper build` first",
                object.display()
            );
            println!("cargo:rustc-link-arg={}", object.display());
            println!("cargo:rerun-if-changed={}", object.display());
        }
    }

    let stamp = build_inputs_dir.join("stamp");
    println!("cargo:rerun-if-changed={}", stamp.display());
}
