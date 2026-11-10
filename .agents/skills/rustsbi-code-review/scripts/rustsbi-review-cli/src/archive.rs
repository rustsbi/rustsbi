//! Archive a review and its sidecars in a timestamped subdirectory.

use anyhow::{Context, Result};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub fn archive_main(argv: &[String]) -> i32 {
    if argv.len() != 1 {
        eprintln!("usage: archive <output-file>");
        return 2;
    }
    match archive(Path::new(&argv[0])) {
        Ok((dest, count)) => {
            println!("{}: {} files archived", dest.display(), count);
            0
        }
        Err(error) => {
            eprintln!("error: {error:#}");
            2
        }
    }
}

fn archive(output: &Path) -> Result<(PathBuf, usize)> {
    let output = std::path::absolute(output)?;
    let mut suffixes: Vec<String> = [
        "",
        ".meta",
        ".input.md",
        ".checks.json",
        ".checks.md",
        ".comments.json",
        ".comments.json.usage.json",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for persona in
        std::iter::once("combined").chain(crate::skillenv::PERSONAS.iter().map(|(name, _)| *name))
    {
        suffixes.push(format!(".comments.json.{persona}.json"));
        suffixes.push(format!(".comments.json.{persona}.json.usage.json"));
    }

    // Select known files before creating the archive. Do not traverse the
    // output's parent or follow artifact symlinks.
    let mut artifacts = Vec::new();
    for suffix in suffixes {
        let mut path = output.as_os_str().to_os_string();
        path.push(&suffix);
        let path = PathBuf::from(path);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                anyhow::ensure!(
                    metadata.is_file(),
                    "{} is not a regular file",
                    path.display()
                );
                artifacts.push(path);
            }
            Err(error) if error.kind() == ErrorKind::NotFound && !suffix.is_empty() => {}
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        }
    }

    let run_dir = output.parent().context("output has no parent directory")?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let dest = create_archive_dir(run_dir, &stamp)?;
    for path in &artifacts {
        std::fs::copy(
            path,
            dest.join(path.file_name().context("artifact has no filename")?),
        )
        .with_context(|| format!("copy {}", path.display()))?;
    }
    Ok((dest, artifacts.len()))
}

fn create_archive_dir(run_dir: &Path, stamp: &str) -> Result<PathBuf> {
    let mut suffix = 1;
    loop {
        let name = if suffix == 1 {
            stamp.to_string()
        } else {
            format!("{stamp}-{suffix}")
        };
        let dest = run_dir.join(name);
        match std::fs::create_dir(&dest) {
            Ok(()) => return Ok(dest),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => suffix += 1,
            Err(error) => return Err(error).with_context(|| format!("mkdir {}", dest.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_only_the_selected_review_and_its_sidecars() -> Result<()> {
        let repo = tempfile::tempdir()?;
        let files = [
            "review.md",
            "review.md.meta",
            "review.md.input.md",
            "review.md.checks.json",
            "review.md.checks.md",
            "review.md.comments.json",
            "review.md.comments.json.usage.json",
            "review.md.comments.json.combined.json",
            "review.md.comments.json.combined.json.usage.json",
            "review.md.comments.json.development.json",
            "review.md.comments.json.development.json.usage.json",
        ];
        for name in files {
            std::fs::write(repo.path().join(name), name)?;
        }
        for name in [
            "Cargo.toml",
            "other-review.md",
            "other-review.md.comments.json",
            "review.md.backup",
            "review.md.comments.json.unrelated.json",
        ] {
            std::fs::write(repo.path().join(name), "unrelated")?;
        }
        for name in [".git", "src", "20260101-000000"] {
            let dir = repo.path().join(name);
            std::fs::create_dir(&dir)?;
            std::fs::write(dir.join("keep.txt"), "unrelated")?;
        }

        let (dest, count) = archive(&repo.path().join("review.md"))?;
        assert_eq!(count, files.len());
        assert_eq!(std::fs::read_dir(&dest)?.count(), files.len());
        for name in files {
            assert_eq!(std::fs::read_to_string(dest.join(name))?, name);
            assert_eq!(std::fs::read_to_string(repo.path().join(name))?, name);
        }
        assert_eq!(
            std::fs::read_to_string(repo.path().join("src/keep.txt"))?,
            "unrelated"
        );
        Ok(())
    }

    #[test]
    fn missing_review_does_not_create_an_archive() -> Result<()> {
        let repo = tempfile::tempdir()?;
        assert!(archive(&repo.path().join("missing.md")).is_err());
        assert_eq!(std::fs::read_dir(repo.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn repeated_archives_preserve_existing_snapshots() -> Result<()> {
        let repo = tempfile::tempdir()?;
        let first = create_archive_dir(repo.path(), "20260101-000000")?;
        std::fs::write(first.join("review.md"), "first")?;
        let second = create_archive_dir(repo.path(), "20260101-000000")?;
        assert_eq!(second.file_name().unwrap(), "20260101-000000-2");
        assert_eq!(std::fs::read_to_string(first.join("review.md"))?, "first");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_sidecars() -> Result<()> {
        let repo = tempfile::tempdir()?;
        let other = tempfile::NamedTempFile::new()?;
        std::fs::write(repo.path().join("review.md"), "review")?;
        std::os::unix::fs::symlink(other.path(), repo.path().join("review.md.input.md"))?;
        assert!(archive(&repo.path().join("review.md")).is_err());
        assert_eq!(std::fs::read_dir(repo.path())?.count(), 2);
        Ok(())
    }
}
