use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Put the linker script somewhere the linker can find it
    fs::File::create(out_dir.join("linker64.ld"))
        .unwrap()
        .write_all(include_bytes!("src/linker64.ld"))
        .unwrap();
    fs::File::create(out_dir.join("linker32.ld"))
        .unwrap()
        .write_all(include_bytes!("src/linker32.ld"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out_dir.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/linker64.ld");
    println!("cargo:rerun-if-changed=src/linker32.ld");
}
