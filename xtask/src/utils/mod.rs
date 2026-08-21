pub mod cargo;

#[macro_use]
pub mod envs;

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Workspace root directory (parent directory of the xtask crate).
///
/// xtask is `publish = false` and always built from this workspace, so the
/// compile-time `CARGO_MANIFEST_DIR/..` is the workspace root regardless of
/// the caller's working directory.
pub(crate) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate must live one level below the workspace root")
        .to_path_buf()
}

/// Cargo target directory: `CARGO_TARGET_DIR` when set (absolutized against
/// the process working directory), otherwise `<workspace_root>/target`.
pub(crate) fn cargo_target_dir() -> PathBuf {
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cargo_target_dir_in(env::var_os("CARGO_TARGET_DIR"), &current_dir)
}

/// Pure core of [`cargo_target_dir`], taking the environment value and the
/// process working directory as parameters so tests do not mutate process
/// state.
pub(crate) fn cargo_target_dir_in(target_dir_env: Option<OsString>, current_dir: &Path) -> PathBuf {
    match target_dir_env {
        Some(dir) if !dir.is_empty() => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                current_dir.join(dir)
            }
        }
        _ => workspace_root().join("target"),
    }
}

pub trait CmdOptional {
    fn optional(&mut self, pred: bool, f: impl FnOnce(&mut Self) -> &mut Self) -> &mut Self {
        if pred {
            f(self);
        }
        self
    }
}
