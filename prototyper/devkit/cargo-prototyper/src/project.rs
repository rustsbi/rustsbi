use std::fs;
use std::path::{Path, PathBuf};

use crate::Error;

/// The checked RustSBI workspace consumed by the development kit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    root: PathBuf,
}

impl Project {
    /// Finds the nearest ancestor containing the RustSBI workspace and
    /// Prototyper product directory.
    pub fn discover(start: impl AsRef<Path>) -> Result<Self, Error> {
        let start = start.as_ref();
        let absolute =
            fs::canonicalize(start).map_err(|error| Error::io("read workspace", error))?;
        for candidate in absolute.ancestors() {
            if candidate.join("Cargo.toml").is_file()
                && candidate.join("prototyper/prototyper/Cargo.toml").is_file()
                && candidate.join("xtask/Cargo.toml").is_file()
            {
                return Ok(Self {
                    root: candidate.to_path_buf(),
                });
            }
        }
        Err(Error::WorkspaceNotFound(absolute))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn target_dir(&self, target: &str, release: bool) -> PathBuf {
        let target_name = Path::new(target)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(target);
        self.root
            .join("target")
            .join(target_name)
            .join(if release { "release" } else { "debug" })
    }

    /// Resolves one user-supplied input path inside or outside the workspace.
    pub fn input(&self, path: &Path) -> Result<PathBuf, Error> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        path.canonicalize().map_err(|_| Error::MissingInput(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_the_checked_workspace_from_the_devkit_package() {
        let project = Project::discover(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(project.root().join("prototyper/machine").is_dir());
    }

    #[test]
    fn custom_target_paths_use_the_file_stem_for_cargo_output() {
        let project = Project::discover(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(
            project
                .target_dir("/tmp/riscv-machine.json", true)
                .ends_with("target/riscv-machine/release")
        );
    }
}
