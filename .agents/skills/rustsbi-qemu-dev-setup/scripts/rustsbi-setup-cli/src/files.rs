// SPDX-License-Identifier: MulanPSL-2.0

use anyhow::{Context, Result, bail, ensure};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

pub fn absolute(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            part => result.push(part),
        }
    }
    Ok(result)
}

pub fn workspace(path: &Path) -> bool {
    [
        "Cargo.toml",
        "rust-toolchain.toml",
        "firmware/prototyper/Cargo.toml",
        "library/rustsbi/Cargo.toml",
    ]
    .iter()
    .all(|p| path.join(p).is_file())
}

pub fn check_source(target: &Path, source: Option<&Path>) -> Result<bool> {
    if let Ok(meta) = fs::symlink_metadata(target) {
        ensure!(
            !meta.is_symlink(),
            "refusing symlink destination: {}",
            target.display()
        );
        ensure!(
            source.is_none() && workspace(target),
            "destination already exists: {}",
            target.display()
        );
        return Ok(true);
    }
    if let Some(source) = source {
        ensure!(source.exists(), "local source is missing");
        ensure!(
            source.is_file() || source.is_dir(),
            "source must be a directory or regular archive file"
        );
        if source.is_dir() {
            // Resolve existing ancestors so a symlinked parent cannot hide recursion.
            let mut ancestor = target;
            while !ancestor.exists() {
                ancestor = ancestor.parent().context("no existing ancestor")?;
            }
            ensure!(
                !ancestor.canonicalize()?.starts_with(source.canonicalize()?),
                "destination is inside local source"
            );
        }
    }
    Ok(false)
}

pub fn publish(source: &Path, target: &Path) -> Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        target,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .with_context(|| format!("cannot publish without replacement: {}", target.display()))?;
    Ok(())
}

pub fn unpack(archive: &Path, target: &Path, qemu: bool) -> Result<PathBuf> {
    fs::create_dir(target)?;
    let file = fs::File::open(archive)?;
    let name = archive.to_string_lossy();
    let input: Box<dyn Read> = if name.ends_with(".xz") {
        Box::new(xz2::read::XzDecoder::new(file))
    } else if name.ends_with(".gz") || name.ends_with(".tgz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    for entry in tar::Archive::new(input).entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let parts: Vec<_> = path.components().collect();
        ensure!(
            !path.is_absolute() && !parts.contains(&Component::ParentDir),
            "unsafe archive path"
        );
        if qemu
            && parts.windows(2).any(|p| {
                p == [
                    Component::Normal("roms".as_ref()),
                    Component::Normal("edk2".as_ref()),
                ]
            })
        {
            continue;
        }
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link(),
            "unsupported archive entry"
        );
        if let Some(link) = entry.link_name()? {
            ensure!(!link.is_absolute(), "absolute archive link");
            let base = if kind.is_symlink() {
                path.parent().unwrap_or(Path::new(""))
            } else {
                Path::new("")
            };
            let mut depth = 0_i64;
            for part in base.join(link).components() {
                match part {
                    Component::Normal(_) => depth += 1,
                    Component::ParentDir => depth -= 1,
                    _ => {}
                }
                ensure!(depth >= 0, "archive link escapes extraction root");
            }
        }
        ensure!(
            entry.unpack_in(target)?,
            "archive entry escapes extraction root"
        );
    }
    let entries = fs::read_dir(target)?.collect::<std::io::Result<Vec<_>>>()?;
    Ok(if entries.len() == 1 && entries[0].file_type()?.is_dir() {
        entries[0].path()
    } else {
        target.to_owned()
    })
}

pub fn prepare_source(target: &Path, source: Option<&Path>) -> Result<()> {
    if check_source(target, source)? {
        return Ok(());
    }
    let parent = target.parent().context("missing source parent")?;
    fs::create_dir_all(parent)?;
    let temp = tempfile::Builder::new()
        .prefix(".rustsbi-source-")
        .tempdir_in(parent)?;
    let stage = temp.path().join("source");
    let staged = if let Some(source) = source {
        if source.is_dir() {
            for entry in WalkDir::new(source).follow_links(false) {
                let entry = entry?;
                let dest = stage.join(entry.path().strip_prefix(source)?);
                if entry.file_type().is_dir() {
                    fs::create_dir_all(&dest)?;
                } else if entry.file_type().is_symlink() {
                    std::os::unix::fs::symlink(fs::read_link(entry.path())?, dest)?;
                } else {
                    fs::copy(entry.path(), dest)?;
                }
            }
            stage
        } else {
            unpack(source, &stage, false)?
        }
    } else {
        ensure!(
            Command::new("git")
                .args(["clone", "https://github.com/rustsbi/rustsbi.git"])
                .arg(&stage)
                .status()?
                .success(),
            "git clone failed"
        );
        stage
    };
    ensure!(
        workspace(&staged),
        "source does not contain a RustSBI workspace"
    );
    publish(&staged, target)
}

pub fn install_prefix(stage: &Path, prefix: &Path) -> Result<()> {
    ensure!(
        fs::symlink_metadata(prefix).is_err(),
        "QEMU prefix already exists"
    );
    let payload = stage.join(prefix.strip_prefix("/")?);
    ensure!(
        !fs::symlink_metadata(&payload)?.is_symlink(),
        "staged prefix is a symlink"
    );
    ensure!(
        payload.join("bin/qemu-system-riscv64").is_file(),
        "staged QEMU missing"
    );
    let mut entries = Vec::new();
    for entry in WalkDir::new(stage).follow_links(false) {
        let entry = entry?;
        let path = entry.path();
        if path == payload {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(&payload) {
            if entry.file_type().is_symlink() {
                let link = fs::read_link(path)?;
                // Installed links must stay within the private prefix.
                ensure!(
                    absolute(&path.parent().unwrap().join(&link))?.starts_with(&payload),
                    "installed link escapes prefix"
                );
            }
            let suffix = if entry.file_type().is_dir() { "/" } else { "" };
            entries.push(format!("{}{suffix}", relative.display()));
        } else if !entry.file_type().is_dir() || !payload.starts_with(path) {
            bail!("installation escapes prefix: {}", path.display());
        }
    }
    ensure!(
        fs::symlink_metadata(payload.join("install-manifest.txt")).is_err(),
        "manifest already exists"
    );
    entries.sort();
    entries.push("install-manifest.txt".into());
    fs::write(
        payload.join("install-manifest.txt"),
        entries.join("\n") + "\n",
    )?;
    publish(&payload, prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_sources_publish_once_and_existing_workspaces_are_reused() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("source");
        let names = [
            "Cargo.toml",
            "rust-toolchain.toml",
            "firmware/prototyper/Cargo.toml",
            "library/rustsbi/Cargo.toml",
        ];
        for name in names {
            let path = source.join(name);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, "source")?;
        }
        let target = temp.path().join("copy");
        prepare_source(&target, Some(&source))?;
        fs::write(target.join("local-work"), "keep")?;
        prepare_source(&target, None)?;
        assert!(prepare_source(&target, Some(&source)).is_err());
        assert_eq!(fs::read_to_string(target.join("local-work"))?, "keep");
        assert!(check_source(&source.join("nested/copy"), Some(&source)).is_err());
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&source, &alias)?;
        assert!(check_source(&alias.join("nested/copy"), Some(&source)).is_err());
        let archive = temp.path().join("source.tar.gz");
        let output = flate2::write::GzEncoder::new(
            fs::File::create(&archive)?,
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(output);
        builder.append_dir_all("rustsbi", &source)?;
        builder.into_inner()?.finish()?;
        let archived = temp.path().join("archived");
        prepare_source(&archived, Some(&archive))?;
        assert!(workspace(&archived));
        Ok(())
    }
    #[test]
    fn publication_never_replaces_a_racing_destination() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        fs::create_dir(&source)?;
        fs::create_dir(&target)?;
        fs::write(target.join("sentinel"), "keep")?;
        assert!(publish(&source, &target).is_err());
        assert_eq!(fs::read_to_string(target.join("sentinel"))?, "keep");
        Ok(())
    }
    #[test]
    fn existing_empty_and_symlink_destinations_are_rejected() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let target = temp.path().join("target");
        fs::create_dir(&target)?;
        assert!(check_source(&target, None).is_err());
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&target, &link)?;
        assert!(check_source(&link, None).is_err());
        Ok(())
    }
    #[test]
    fn archive_link_cannot_write_outside_target() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let archive = temp.path().join("bad.tar");
        let mut builder = tar::Builder::new(fs::File::create(&archive)?);
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o777);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name("../outside")?;
        header.set_cksum();
        builder.append_data(&mut header, "link", std::io::empty())?;
        builder.finish()?;
        assert!(unpack(&archive, &temp.path().join("extract"), false).is_err());
        assert!(!temp.path().join("outside").exists());
        Ok(())
    }
    #[test]
    fn private_install_rejects_side_payload() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let stage = temp.path().join("stage");
        let prefix = temp.path().join("qemu");
        let payload = stage.join(prefix.strip_prefix("/")?);
        fs::create_dir_all(payload.join("bin"))?;
        fs::write(payload.join("bin/qemu-system-riscv64"), "binary")?;
        fs::write(stage.join("unexpected"), "side effect")?;
        assert!(install_prefix(&stage, &prefix).is_err());
        assert!(!prefix.exists());
        Ok(())
    }
}
