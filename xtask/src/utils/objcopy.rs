use std::{
    ffi::OsStr,
    io,
    path::Path,
    process::{Command, ExitStatus},
};

use anyhow::{Context, Result};

/// Convert an ELF file to a raw binary, preferring the Rust wrapper and
/// falling back to LLVM only when the wrapper executable cannot be found.
pub fn elf_to_binary(elf: &Path, binary: &Path) -> Result<ExitStatus> {
    elf_to_binary_with(
        OsStr::new("rust-objcopy"),
        OsStr::new("llvm-objcopy"),
        elf,
        binary,
    )
}

fn elf_to_binary_with(
    rust_objcopy: &OsStr,
    llvm_objcopy: &OsStr,
    elf: &Path,
    binary: &Path,
) -> Result<ExitStatus> {
    let args = [
        OsStr::new("-O"),
        OsStr::new("binary"),
        elf.as_os_str(),
        binary.as_os_str(),
    ];
    match Command::new(rust_objcopy).args(args).status() {
        Ok(status) => Ok(status),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Command::new(llvm_objcopy)
            .args(args)
            .status()
            .with_context(|| {
                format!(
                    "failed to execute llvm-objcopy (rust-objcopy was not found): '{}' -> '{}'",
                    elf.display(),
                    binary.display()
                )
            }),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to execute rust-objcopy: '{}' -> '{}'",
                elf.display(),
                binary.display()
            )
        }),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use tempfile::tempdir;

    use super::elf_to_binary_with;

    fn executable(dir: &Path, name: &str, exit_code: i32) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n[ \"$1\" = \"-O\" ] || exit 20\n[ \"$2\" = \"binary\" ] || exit 21\n[ \"$3\" = \"input.elf\" ] || exit 22\n[ \"$4\" = \"output.bin\" ] || exit 23\nexit {exit_code}\n")).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[test]
    fn prefers_rust_objcopy_when_both_tools_are_available() {
        let dir = tempdir().unwrap();
        let rust = executable(dir.path(), "rust-objcopy", 0);
        let llvm = executable(dir.path(), "llvm-objcopy", 24);

        let status = elf_to_binary_with(
            rust.as_os_str(),
            llvm.as_os_str(),
            Path::new("input.elf"),
            Path::new("output.bin"),
        )
        .unwrap();

        assert!(status.success());
    }

    #[test]
    fn falls_back_to_llvm_only_when_rust_objcopy_is_missing() {
        let dir = tempdir().unwrap();
        let missing_rust = dir.path().join("missing-rust-objcopy");
        let llvm = executable(dir.path(), "llvm-objcopy", 0);

        let status = elf_to_binary_with(
            missing_rust.as_os_str(),
            llvm.as_os_str(),
            Path::new("input.elf"),
            Path::new("output.bin"),
        )
        .unwrap();

        assert!(status.success());
    }

    #[test]
    fn does_not_fall_back_after_rust_objcopy_returns_failure() {
        let dir = tempdir().unwrap();
        let rust = executable(dir.path(), "rust-objcopy", 25);
        let llvm = executable(dir.path(), "llvm-objcopy", 0);

        let status = elf_to_binary_with(
            rust.as_os_str(),
            llvm.as_os_str(),
            Path::new("input.elf"),
            Path::new("output.bin"),
        )
        .unwrap();

        assert!(!status.success());
    }

    #[test]
    fn reports_missing_llvm_tool_after_rust_tool_is_missing() {
        let dir = tempdir().unwrap();
        let missing_rust = dir.path().join("missing-rust-objcopy");
        let missing_llvm = dir.path().join("missing-llvm-objcopy");

        let error = elf_to_binary_with(
            missing_rust.as_os_str(),
            missing_llvm.as_os_str(),
            Path::new("input.elf"),
            Path::new("output.bin"),
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("failed to execute llvm-objcopy"));
    }
}
