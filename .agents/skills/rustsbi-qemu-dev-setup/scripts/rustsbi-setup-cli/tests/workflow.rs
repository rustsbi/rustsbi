// SPDX-License-Identifier: MulanPSL-2.0

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for file in [
        "Cargo.toml",
        "rust-toolchain.toml",
        "firmware/prototyper/Cargo.toml",
        "library/rustsbi/Cargo.toml",
    ] {
        write(&temp.path().join("repo").join(file), "fixture");
    }
    for name in [
        "git",
        "curl",
        "cc",
        "rustup",
        "cargo",
        "rust-objcopy",
        "qemu-system-riscv64",
    ] {
        let script = match name {
            "qemu-system-riscv64" => "echo 'QEMU emulator version 10.2.1'",
            "cargo" => {
                r#"
echo "$*" >> "$HOME/cargo-calls"
if [ "$*" = 'prototyper build' ]; then
  mkdir -p target/riscv64gc-unknown-none-elf/release
  echo ELF > target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf
  if [ -z "$MISSING_BIN" ]; then
    echo BIN > target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin
  fi
fi
"#
            }
            "curl" | "rustup" => "echo forbidden >> \"$HOME/forbidden\"; exit 97",
            _ => "exit 0",
        };
        let path = temp.path().join("tools").join(name);
        write(&path, &format!("#!/bin/sh\n{script}\n"));
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    temp
}
fn command(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rustsbi-setup-cli"));
    cmd.env("HOME", root)
        .env("CARGO_HOME", root.join("cargo"))
        .env("RUSTUP_HOME", root.join("rustup"))
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("tools").display()),
        );
    cmd
}
#[test]
fn planning_does_not_run_rust_proxies_or_write_outputs() {
    let temp = fixture();
    for action in ["preflight", "setup"] {
        let out = command(temp.path())
            .arg(action)
            .arg("--plan")
            .arg("--repo-dir")
            .arg(temp.path().join("repo"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!temp.path().join("cargo-calls").exists());
        assert!(!temp.path().join("forbidden").exists());
        assert!(!temp.path().join("repo/target").exists());
    }
}
#[test]
fn setup_needs_authorization() {
    let temp = fixture();
    assert!(
        !command(temp.path())
            .args(["setup", "--repo-dir"])
            .arg(temp.path().join("repo"))
            .status()
            .unwrap()
            .success()
    );
    assert!(!temp.path().join("cargo-calls").exists());
}

#[test]
fn relative_path_tools_remain_usable_after_changing_to_the_repository() {
    let temp = fixture();
    let output = command(temp.path())
        .current_dir(temp.path())
        .env("PATH", "tools:/usr/bin:/bin")
        .args(["setup", "--yes", "--repo-dir"])
        .arg(temp.path().join("repo"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("boot_verified: true"));
}
#[test]
fn missing_binary_blocks_success() {
    let temp = fixture();
    let output = command(temp.path())
        .env("MISSING_BIN", "1")
        .args(["setup", "--yes", "--repo-dir"])
        .arg(temp.path().join("repo"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        !fs::read_to_string(temp.path().join("cargo-calls"))
            .unwrap()
            .contains("prototyper test")
    );
}
#[test]
fn successful_boot_and_compile_only_are_distinct() {
    let temp = fixture();
    for (options, expected) in [(vec![], "true"), (vec!["--no-test"], "false")] {
        let output = command(temp.path())
            .args(["setup", "--yes", "--repo-dir"])
            .arg(temp.path().join("repo"))
            .args(options)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(&format!("boot_verified: {expected}"))
        );
        assert!(!temp.path().join("forbidden").exists());
    }
}

#[test]
fn source_install_audits_payload_and_isolates_every_build_command() {
    let temp = fixture();
    let root = temp.path();
    fs::remove_file(root.join("tools/qemu-system-riscv64")).unwrap();
    let assert_cache = r#"
case "$XDG_CACHE_HOME:$CCACHE_DIR:$SCCACHE_DIR" in
  "$TMPDIR"/*:"$TMPDIR"/*:"$TMPDIR"/*) ;;
  *) exit 99 ;;
esac
"#;
    for name in ["c++", "ninja", "meson", "pkg-config", "make"] {
        let body = if name == "make" {
            r#"
if [ "$2" = install ]; then
  dest=${1#DESTDIR=}
  prefix=$(cat prefix.txt)
  payload=$dest$prefix
  mkdir -p "$payload/bin" "$payload/include" "$payload/lib"
  printf '#!/bin/sh\necho "QEMU emulator version 10.2.1"\n' > "$payload/bin/qemu-system-riscv64"
  chmod +x "$payload/bin/qemu-system-riscv64"
  echo header > "$payload/include/fdt.h"
  echo library > "$payload/lib/libfdt.a"
fi
"#
        } else {
            "exit 0"
        };
        let path = root.join("tools").join(name);
        write(&path, &format!("#!/bin/sh\n{assert_cache}\n{body}\n"));
        // Native prerequisite probes are read-only and do not use the build cache.
        if name != "make" {
            write(&path, "#!/bin/sh\nexit 0\n");
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let archive = root.join("qemu.tar");
    let mut builder = tar::Builder::new(fs::File::create(&archive).unwrap());
    let configure = format!(
        "#!/bin/sh\n{assert_cache}\nfor arg in \"$@\"; do case \"$arg\" in --prefix=*) printf '%s' \"${{arg#--prefix=}}\" > prefix.txt ;; esac; done\n"
    );
    for (name, bytes, mode) in [
        ("qemu/configure", configure.as_bytes(), 0o755),
        ("qemu/meson.build", b"fixture".as_slice(), 0o644),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(mode);
        header.set_cksum();
        builder.append_data(&mut header, name, bytes).unwrap();
    }
    builder.finish().unwrap();
    drop(builder);
    let prefix = root.join("private/qemu");
    let output = command(root)
        .env("TMPDIR", root)
        .args([
            "setup",
            "--yes",
            "--no-test",
            "--qemu-from",
            "source",
            "--qemu-archive",
        ])
        .arg(archive)
        .arg("--qemu-prefix")
        .arg(&prefix)
        .arg("--repo-dir")
        .arg(root.join("repo"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(prefix.join("install-manifest.txt")).unwrap();
    assert!(manifest.contains("include/fdt.h"));
    assert!(manifest.contains("lib/libfdt.a"));
    assert!(!fs::read_dir(root).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("rustsbi-qemu-")
    }));
}
