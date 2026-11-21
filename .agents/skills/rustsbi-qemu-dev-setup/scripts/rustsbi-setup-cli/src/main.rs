// SPDX-License-Identifier: MulanPSL-2.0

mod files;

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, ValueEnum};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    Auto,
    Distro,
    Source,
}

#[derive(Parser, Debug)]
#[command(about = "Preflight, plan and provision a private Linux RustSBI environment")]
struct Args {
    #[arg(value_parser = ["preflight", "setup"])]
    action: String,
    #[arg(long)]
    repo_dir: Option<PathBuf>,
    #[arg(long)]
    local_copy: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "auto")]
    qemu_from: Mode,
    #[arg(long, default_value = "10.2.1")]
    qemu_version: String,
    #[arg(long)]
    qemu_prefix: Option<PathBuf>,
    #[arg(long)]
    qemu_archive: Option<PathBuf>,
    #[arg(long)]
    no_test: bool,
    #[arg(long)]
    plan: bool,
    #[arg(long)]
    yes: bool,
}

fn home(name: &str, default: &Path) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| default.into())
}
fn tool(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join(name))
        .find(|p| {
            use std::os::unix::fs::PermissionsExt;
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
        .and_then(|path| files::absolute(&path).ok())
}
fn run(command: &mut Command) -> Result<()> {
    println!("Running: {command:?}");
    ensure!(command.status()?.success(), "command failed: {command:?}");
    Ok(())
}
fn download(url: &str, dest: &Path) -> Result<()> {
    run(Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "--retry",
            "2",
            "-fL",
            url,
            "-o",
        ])
        .arg(dest))
}
fn version(path: &Path) -> Result<String> {
    let out = Command::new(path).arg("--version").output()?;
    ensure!(out.status.success(), "QEMU version probe failed");
    let text = String::from_utf8(out.stdout)?;
    Ok(text
        .split_whitespace()
        .nth(3)
        .context("invalid QEMU version output")?
        .to_owned())
}
fn usable(value: &str) -> bool {
    let parts = value
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>();
    parts.is_ok_and(|v| v.len() == 3 && v.as_slice() >= [9, 1, 0].as_slice() && value != "10.2.4")
}

fn execute(mut args: Args) -> Result<u8> {
    ensure!(env::consts::OS == "linux", "Linux is required");
    ensure!(
        usable(&args.qemu_version),
        "unsupported QEMU source version"
    );
    let user_home = PathBuf::from(env::var_os("HOME").context("HOME is required")?);
    let repo = files::absolute(
        &args
            .repo_dir
            .take()
            .unwrap_or_else(|| user_home.join("rustsbi")),
    )?;
    let source = args
        .local_copy
        .as_deref()
        .map(files::absolute)
        .transpose()?;
    files::check_source(&repo, source.as_deref())?;
    let prefix = files::absolute(&args.qemu_prefix.take().unwrap_or_else(|| {
        user_home
            .join(".local/share/rustsbi/qemu")
            .join(&args.qemu_version)
    }))?;
    ensure!(
        prefix != Path::new("/") && prefix.file_name().is_some(),
        "choose a private QEMU prefix"
    );
    if let Some(archive) = &args.qemu_archive {
        ensure!(archive.is_file(), "QEMU archive missing");
    }
    let cargo_home = files::absolute(&home("CARGO_HOME", &user_home.join(".cargo")))?;
    let rustup_home = files::absolute(&home("RUSTUP_HOME", &user_home.join(".rustup")))?;
    let tools_root = user_home.join(".local/share/rustsbi/cargo-tools");
    let mut qemu = if !matches!(args.qemu_from, Mode::Source) {
        tool("qemu-system-riscv64").filter(|p| version(p).is_ok_and(|v| usable(&v)))
    } else {
        None
    };
    if qemu.is_none() && matches!(args.qemu_from, Mode::Distro) {
        bail!("no usable QEMU on PATH (>=9.1, excluding 10.2.4)");
    }
    if qemu.is_none() && fs::symlink_metadata(&prefix).is_ok() {
        ensure!(
            !fs::symlink_metadata(&prefix)?.is_symlink()
                && prefix.join("install-manifest.txt").is_file(),
            "unmanaged QEMU prefix exists"
        );
        let binary = prefix.join("bin/qemu-system-riscv64");
        ensure!(
            version(&binary)? == args.qemu_version,
            "private QEMU version mismatch"
        );
        qemu = Some(binary);
    }
    let mut blocked = false;
    let mut native = vec!["git", "curl", "cc"];
    if qemu.is_none() {
        native.extend(["c++", "make", "ninja", "meson", "pkg-config"]);
    }
    for name in native {
        if tool(name).is_some() {
            println!("PASS host:{name}");
        } else {
            println!("BLOCK host:{name} missing; prepare native prerequisites");
            blocked = true;
        }
    }
    if qemu.is_none()
        && !Command::new("pkg-config")
            .args(["--exists", "glib-2.0", "gthread-2.0"])
            .status()
            .is_ok_and(|s| s.success())
    {
        println!("BLOCK host:glib development files missing");
        blocked = true;
    }
    let mut fixable = qemu.is_none();
    for name in ["rustup", "cargo", "rust-objcopy"] {
        let present = tool(name).is_some()
            || cargo_home.join("bin").join(name).is_file()
            || tools_root.join("bin").join(name).is_file();
        println!(
            "{} {name} (not executed)",
            if present { "PASS" } else { "FIX" }
        );
        fixable |= !present;
    }
    println!("Repository: {}", repo.display());
    println!(
        "Source: {}",
        if source.is_some() {
            "stage local copy; publish without replacement"
        } else if repo.exists() {
            "reuse workspace"
        } else {
            "clone https://github.com/rustsbi/rustsbi.git"
        }
    );
    println!(
        "QEMU: {}",
        qemu.as_ref()
            .map(|p| format!("reuse {}", p.display()))
            .unwrap_or_else(|| format!("build {} into {}", args.qemu_version, prefix.display()))
    );
    println!(
        "Rust homes: {} and {}",
        rustup_home.display(),
        cargo_home.display()
    );
    println!(
        "cargo-binutils: {}; outputs: {}/target",
        tools_root.display(),
        repo.display()
    );
    println!("Temporary source/build/cache in system temp; QEMU staged beside prefix");
    println!("System package managers and shell startup files: no writes");
    if args.action == "preflight" {
        println!(
            "VERDICT: {}",
            if blocked {
                "BLOCKED"
            } else if fixable {
                "FIXABLE"
            } else {
                "READY"
            }
        );
        return Ok(if blocked {
            3
        } else if fixable {
            2
        } else {
            0
        });
    }
    if args.plan {
        return Ok(if blocked { 3 } else { 0 });
    }
    ensure!(!blocked, "prepare native dependencies before setup");
    ensure!(
        args.yes,
        "review the write inventory and authorize setup before passing --yes"
    );
    files::prepare_source(&repo, source.as_deref())?;
    if qemu.is_none() {
        let work = tempfile::Builder::new().prefix("rustsbi-qemu-").tempdir()?;
        let archive = if let Some(path) = &args.qemu_archive {
            files::absolute(path)?
        } else {
            let path = work.path().join("qemu.tar.xz");
            download(
                &format!(
                    "https://download.qemu.org/qemu-{}.tar.xz",
                    args.qemu_version
                ),
                &path,
            )?;
            path
        };
        let source = files::unpack(&archive, &work.path().join("source"), true)?;
        ensure!(
            source.join("configure").is_file() && source.join("meson.build").is_file(),
            "invalid QEMU source archive"
        );
        let build = work.path().join("build");
        fs::create_dir(&build)?;
        fs::create_dir_all(prefix.parent().context("no prefix parent")?)?;
        let stage = tempfile::Builder::new()
            .prefix(".rustsbi-qemu-stage-")
            .tempdir_in(prefix.parent().unwrap())?;
        let qemu_command = |program: &Path| {
            let mut command = Command::new(program);
            command.current_dir(&build);
            for (name, dir) in [
                ("XDG_CACHE_HOME", "cache"),
                ("CCACHE_DIR", "ccache"),
                ("SCCACHE_DIR", "sccache"),
            ] {
                command.env(name, work.path().join(dir));
            }
            command
        };
        let configure = &mut qemu_command(&source.join("configure"));
        configure
            .current_dir(&build)
            .arg(format!("--prefix={}", prefix.display()))
            .arg(format!("--sysconfdir={}/etc", prefix.display()))
            .arg(format!("--localstatedir={}/var", prefix.display()))
            .args([
                "--target-list=riscv64-softmmu",
                "--disable-docs",
                "--disable-werror",
                "--disable-tools",
                "--disable-guest-agent",
                "--disable-plugins",
                "--disable-install-blobs",
            ]);
        run(configure)?;
        let jobs = env::var("RUSTSBI_SETUP_JOBS").unwrap_or_else(|_| {
            std::thread::available_parallelism()
                .map_or(2, usize::from)
                .to_string()
        });
        ensure!(
            jobs.parse::<usize>().is_ok_and(|v| v > 0),
            "invalid RUSTSBI_SETUP_JOBS"
        );
        run(qemu_command(Path::new("make")).arg(format!("-j{jobs}")))?;
        run(qemu_command(Path::new("make"))
            .arg(format!("DESTDIR={}", stage.path().display()))
            .arg("install"))?;
        ensure!(
            version(
                &stage
                    .path()
                    .join(prefix.strip_prefix("/")?)
                    .join("bin/qemu-system-riscv64")
            )? == args.qemu_version,
            "staged QEMU version mismatch"
        );
        files::install_prefix(stage.path(), &prefix)?;
        qemu = Some(prefix.join("bin/qemu-system-riscv64"));
    }
    let qemu = qemu.context("no selected QEMU")?;
    let mut paths = vec![
        qemu.parent().unwrap().to_owned(),
        cargo_home.join("bin"),
        tools_root.join("bin"),
    ];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let path = env::join_paths(paths)?;
    // Environment changes apply only to child processes, never the caller's shell.
    let command = |name: &str| {
        let mut cmd = Command::new(name);
        cmd.env("PATH", &path)
            .env("CARGO_HOME", &cargo_home)
            .env("RUSTUP_HOME", &rustup_home)
            .current_dir(&repo);
        cmd
    };
    if !cargo_home.join("bin/rustup").is_file() && tool("rustup").is_none() {
        let temp = tempfile::tempdir()?;
        let installer = temp.path().join("rustup.sh");
        download("https://sh.rustup.rs", &installer)?;
        run(command("sh").arg(installer).args([
            "-y",
            "--profile",
            "minimal",
            "--default-toolchain",
            "none",
            "--no-modify-path",
        ]))?;
    }
    // Absolute cargo path also works when no proxy was present on the original PATH.
    let cargo = if cargo_home.join("bin/cargo").is_file() {
        cargo_home.join("bin/cargo")
    } else {
        tool("cargo").context("cargo missing")?
    };
    let cargo_cmd = || {
        let mut cmd = command(cargo.to_str().unwrap());
        cmd.env("CARGO_TARGET_DIR", repo.join("target"));
        cmd
    };
    run(cargo_cmd().arg("--version"))?;
    if !tools_root.join("bin/rust-objcopy").is_file()
        && !cargo_home.join("bin/rust-objcopy").is_file()
        && tool("rust-objcopy").is_none()
    {
        run(cargo_cmd()
            .args(["install", "--locked", "--root"])
            .arg(&tools_root)
            .arg("cargo-binutils@0.4.0"))?;
    }
    run(cargo_cmd().args(["build", "--locked"]))?;
    run(cargo_cmd().args(["prototyper", "build"]))?;
    for extension in ["elf", "bin"] {
        let artifact = repo.join(format!(
            "target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.{extension}"
        ));
        ensure!(
            artifact
                .metadata()
                .is_ok_and(|m| m.is_file() && m.len() > 0),
            "missing firmware: {}",
            artifact.display()
        );
    }
    if !args.no_test {
        run(cargo_cmd().args(["prototyper", "test"]))?;
    }
    println!(
        "ENVIRONMENT UP\nrepository: {}\nqemu: {}\nboot_verified: {}",
        repo.display(),
        qemu.display(),
        !args.no_test
    );
    Ok(0)
}

fn main() -> ExitCode {
    match execute(Args::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("setup: {error:#}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_policy() {
        assert!(!usable("9.0.9"));
        assert!(usable("9.1.0"));
        assert!(usable("10.2.1"));
        assert!(!usable("10.2.4"));
        assert!(!usable("10.2.1;false"));
    }
}
