use std::{env, fs, path::PathBuf};

fn main() {
    let cargo_out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not set OUT_DIR"));
    let firmware_crate_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not set CARGO_MANIFEST_DIR"),
    );
    let workspace_dir = env::var_os("CARGO_WORKSPACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| firmware_crate_dir.join("../.."));
    let build_inputs_dir = workspace_dir.join("target/prototyper");

    for file_name in [
        "generated_alignment.rs",
        "generated_payload.rs",
        "generated_fdt.rs",
    ] {
        let generated_file = build_inputs_dir.join(file_name);
        let cargo_file = cargo_out_dir.join(file_name);
        fs::copy(&generated_file, &cargo_file).unwrap_or_else(|error| {
            panic!(
                "failed to copy generated firmware source from '{}' to '{}': {error}; \
                 run `cargo prototyper build` first",
                generated_file.display(),
                cargo_file.display(),
            );
        });
    }

    let stamp = build_inputs_dir.join("stamp");
    println!("cargo:rerun-if-changed={}", stamp.display());
}
