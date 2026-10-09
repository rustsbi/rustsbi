//! Linker objects for binary images, without Rust statics or references.

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};

use super::{
    build::BuildMode,
    config::BuildSpec,
    generate::{self, BuildPaths},
};

// An empty object supplies the target's ELF class, architecture and float ABI.
// Converting raw bytes directly with objcopy loses the RISC-V float ABI flags.
const EMPTY_IMAGE_SOURCE: &str = "#![feature(no_core)]\n#![no_core]\n";

pub(super) fn build_image_objects(spec: &BuildSpec, paths: &BuildPaths) -> Result<()> {
    let is_payload_mode = matches!(spec.mode, BuildMode::Payload { .. });
    if !is_payload_mode && spec.fdt.is_none() {
        return Ok(());
    }

    let source = paths.build_inputs_dir.join("image_template.rs");
    let template = paths.build_inputs_dir.join("image_template.o");
    generate::write_if_changed(&source, EMPTY_IMAGE_SOURCE.as_bytes())?;
    let output = Command::new("rustc")
        .args([
            "-Zunstable-options",
            "--crate-type=lib",
            "--emit=obj",
            "--crate-name=embedded_image",
            "--target",
            spec.custom_target
                .as_deref()
                .unwrap_or(spec.target.triple()),
        ])
        .arg(&source)
        .arg("-o")
        .arg(&template)
        .output()
        .context("failed to execute rustc for the embedded-image object template")?;
    if !output.status.success() {
        bail!(
            "rustc failed to create the embedded-image object template: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    if is_payload_mode {
        embed_image(
            &template,
            &paths.payload_data(),
            &paths.payload_object(),
            ".payload",
            4,
        )?;
    }
    if spec.fdt.is_some() {
        embed_image(
            &template,
            &paths.fdt_data(),
            &paths.fdt_object(),
            ".fdt",
            16,
        )?;
    }
    Ok(())
}

fn embed_image(
    template: &Path,
    data: &Path,
    object: &Path,
    section: &str,
    alignment: usize,
) -> Result<()> {
    let pending = object.with_extension("o.pending");
    let mut section_input = std::ffi::OsString::from(format!("--add-section={section}="));
    section_input.push(data);
    let output = Command::new("rust-objcopy")
        .arg(section_input)
        .arg(format!(
            "--set-section-flags={section}=alloc,load,data,contents"
        ))
        .arg("--strip-all")
        .arg(template)
        .arg(&pending)
        .output()
        .with_context(|| {
            format!(
                "failed to execute rust-objcopy for embedded image '{}'; \
                 install cargo-binutils with `cargo install --locked cargo-binutils@0.4.0`",
                data.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "rust-objcopy failed to embed '{}': {}",
            data.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // llvm-objcopy applies alignment changes before adding new sections.
    // A second pass is therefore required to align the inserted section.
    let output = Command::new("rust-objcopy")
        .arg(format!("--set-section-alignment={section}={alignment}"))
        .arg(&pending)
        .output()
        .context("failed to execute rust-objcopy to align the embedded image")?;
    if !output.status.success() {
        bail!(
            "rust-objcopy failed to align '{}': {}",
            data.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let bytes = fs::read(&pending)
        .with_context(|| format!("failed to read generated object '{}'", pending.display()))?;
    generate::write_if_changed(object, &bytes)?;
    fs::remove_file(&pending)
        .with_context(|| format!("failed to remove temporary object '{}'", pending.display()))
}
