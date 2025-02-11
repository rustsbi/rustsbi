use std::{
    ffi::OsStr,
    ops::{Deref, DerefMut},
    path::Path,
    process::Command,
};

use super::CmdOptional;

pub struct Cargo {
    cmd: Command,
}

#[allow(unused)]
impl Cargo {
    pub fn new(action: &str) -> Self {
        let mut cmd = Command::new(env!("CARGO"));
        cmd.arg(action);
        Self { cmd }
    }

    pub fn package<S: AsRef<OsStr>>(&mut self, package: S) -> &mut Self {
        self.args(["--package", package.as_ref().to_str().unwrap()]);
        self
    }

    pub fn work_dir<S: AsRef<Path>>(&mut self, dir: S) -> &mut Self {
        self.current_dir(dir);
        self
    }

    pub fn release(&mut self) -> &mut Self {
        self.arg("--release");
        self
    }

    pub fn target<S: AsRef<OsStr>>(&mut self, target: S) -> &mut Self {
        self.args(["--target", target.as_ref().to_str().unwrap()]);
        self
    }

    pub fn features<I, S>(&mut self, features: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args([
            "--features",
            features
                .into_iter()
                .map(|f| f.as_ref().to_str().unwrap().to_string())
                .collect::<Vec<_>>()
                .join(",")
                .as_ref(),
        ]);
        self
    }

    pub fn no_default_features(&mut self) -> &mut Self {
        self.arg("--no-default-features");
        self
    }

    pub fn unstable<I, S>(&mut self, key: S, values: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.arg(format!(
            "-Z{}={}",
            key.as_ref().to_str().unwrap(),
            values
                .into_iter()
                .map(|f| f.as_ref().to_str().unwrap().to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
        self
    }

    pub fn env<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.cmd.env(key, value);
        self
    }
}

impl CmdOptional for Cargo {}

impl Deref for Cargo {
    type Target = Command;

    fn deref(&self) -> &Self::Target {
        &self.cmd
    }
}

impl DerefMut for Cargo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cmd
    }
}
