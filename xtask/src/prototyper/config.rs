use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use super::{
    ARCH,
    build::{BuildArgs, BuildMode},
};

/// A resolved and validated prototyper build.
#[derive(Debug, Clone)]
pub(crate) struct BuildSpec {
    /// Firmware mode; no subcommand on the CLI normalizes to `BuildMode::Dynamic`.
    pub(crate) mode: BuildMode,
    /// FDT path, if one was supplied.
    pub(crate) fdt: Option<PathBuf>,
    /// User-supplied cargo features (mode-affecting names already rejected).
    pub(crate) features: Vec<String>,
    /// Raw `--target` value (target triple or custom target JSON path).
    pub(crate) target: String,
    /// Target triple cargo reports (file stem for a custom target JSON).
    pub(crate) target_triple: String,
    /// Build in the debug profile instead of release.
    pub(crate) debug: bool,
    /// Config file source installed into the build-input directory.
    pub(crate) config_source: PathBuf,
    /// Platform addresses parsed and validated from the active config TOML.
    pub(crate) platform_addresses: PlatformAddresses,
    /// Artifact name suffix.
    pub(crate) artifact_suffix: String,
}

/// Platform addresses parsed from the active config TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlatformAddresses {
    /// Link start of the firmware image.
    pub(crate) link_start_address: u64,
    /// Where the payload section is linked.
    pub(crate) payload_address: u64,
}

/// Resolve raw CLI arguments into a validated build specification.
pub(crate) fn resolve(args: &BuildArgs) -> Result<BuildSpec> {
    let current_dir = env::current_dir().context("failed to determine current directory")?;
    resolve_in(args, &current_dir)
}

pub(crate) fn resolve_in(args: &BuildArgs, current_dir: &Path) -> Result<BuildSpec> {
    let mode = args.mode.clone().unwrap_or(BuildMode::Dynamic);
    let features = normalize_features(&args.features);

    for feature in &features {
        match feature.as_str() {
            "payload" => bail!(
                "feature `payload` cannot be passed via --features; \
                 select payload mode with `cargo prototyper build payload <PATH>` instead"
            ),
            "jump" => bail!(
                "feature `jump` cannot be passed via --features; \
                 select jump mode with `cargo prototyper build jump` instead"
            ),
            "fdt" => bail!(
                "feature `fdt` cannot be passed via --features; \
                 pass the device tree with `--fdt <PATH>` instead"
            ),
            _ => {}
        }
    }

    let mode = match mode {
        BuildMode::Payload { path } => BuildMode::Payload {
            path: absolutize(&path, current_dir),
        },
        other => other,
    };
    let fdt = args.fdt.as_deref().map(|p| absolutize(p, current_dir));

    if let BuildMode::Payload { path } = &mode
        && !path.exists()
    {
        bail!("payload file does not exist: '{}'", path.display());
    }

    if let Some(fdt) = &fdt
        && !fdt.exists()
    {
        bail!("FDT file does not exist: '{}'", fdt.display());
    }

    let config_source = args.config_file.clone().unwrap_or_else(|| {
        current_dir
            .join("prototyper")
            .join("prototyper")
            .join("config")
            .join("default.toml")
    });
    if !config_source.exists() {
        bail!("config file '{}' does not exist", config_source.display());
    }
    let platform_addresses = parse_config(&config_source)?;

    let target = args.target.clone().unwrap_or_else(|| ARCH.to_string());
    let target_triple = get_target_triple(&target);
    let artifact_suffix = default_artifact_suffix(&mode).to_string();

    Ok(BuildSpec {
        mode,
        fdt,
        features,
        target,
        target_triple,
        debug: args.debug,
        config_source,
        platform_addresses,
        artifact_suffix,
    })
}

fn normalize_features(features: &[String]) -> Vec<String> {
    features
        .iter()
        .flat_map(|feature| feature.split(','))
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
        .collect()
}

fn absolutize(path: &Path, current_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    }
}

fn parse_config(config_source: &Path) -> Result<PlatformAddresses> {
    let content = fs::read_to_string(config_source)
        .with_context(|| format!("failed to read config file '{}'", config_source.display()))?;
    let value: toml::Value = toml::from_str(&content).with_context(|| {
        format!(
            "failed to parse config file '{}' as TOML",
            config_source.display()
        )
    })?;

    let address = |key: &str| -> Result<u64> {
        match value.get(key) {
            None => bail!(
                "config '{}' is missing required key `{}`; \
                 the config schema requires `link_start_address`, `payload_address` \
                 and `jump_address` — copy them from `prototyper/prototyper/config/default.toml`",
                config_source.display(),
                key
            ),
            Some(toml::Value::Integer(i)) if *i >= 0 => {
                let address = *i as u64;
                if !address.is_multiple_of(0x1000) {
                    bail!(
                        "address key `{}` in config '{}' must be 0x1000-aligned, got {:#x}",
                        key,
                        config_source.display(),
                        address
                    );
                }
                Ok(address)
            }
            Some(_) => bail!(
                "address key `{}` in config '{}' must be a non-negative integer",
                key,
                config_source.display()
            ),
        }
    };

    let link_start_address = address("link_start_address")?;
    let payload_address = address("payload_address")?;
    address("jump_address")?;

    if link_start_address >= payload_address {
        bail!(
            "invalid platform addresses in config '{}': `link_start_address` ({:#x}) \
             must be less than `payload_address` ({:#x})",
            config_source.display(),
            link_start_address,
            payload_address
        );
    }

    Ok(PlatformAddresses {
        link_start_address,
        payload_address,
    })
}

fn default_artifact_suffix(mode: &BuildMode) -> &'static str {
    match mode {
        BuildMode::Dynamic => "dynamic",
        BuildMode::Jump => "jump",
        BuildMode::Payload { .. } => "payload",
    }
}

fn get_target_triple(target: &str) -> String {
    fn is_target_file(target: &str) -> bool {
        target.ends_with(".json") && Path::new(target).exists()
    }
    if is_target_file(target) {
        Path::new(target)
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("Invalid file path: {}", target))
            .unwrap_or_else(|err| {
                eprintln!("Warning: {}. Falling back to target string.", err);
                target
            })
            .to_string()
    } else {
        target.to_string()
    }
}

impl BuildSpec {
    pub(crate) fn cargo_features(&self) -> Vec<String> {
        let mut features = self.features.clone();
        if self.fdt.is_some() {
            features.push("fdt".to_string());
        }
        match &self.mode {
            BuildMode::Dynamic => {}
            BuildMode::Jump => features.push("jump".to_string()),
            BuildMode::Payload { .. } => features.push("payload".to_string()),
        }
        features
    }

    pub(crate) fn encoded_rustflags(&self, linker_script: &Path) -> String {
        let mut flags = vec![
            "-C".to_string(),
            "relocation-model=pie".to_string(),
            "-C".to_string(),
            "link-arg=-pie".to_string(),
        ];
        if self.features.iter().any(|feature| feature == "hypervisor") {
            flags.extend(["-C".to_string(), "target-feature=+h".to_string()]);
        }
        flags.extend([
            "-C".to_string(),
            format!("link-arg=-T{}", linker_script.display()),
        ]);
        flags.join("\u{1f}")
    }

    pub(crate) fn artifact_suffix(&self) -> &str {
        &self.artifact_suffix
    }

    pub(crate) fn override_artifact_suffix(&mut self, suffix: impl Into<String>) {
        self.artifact_suffix = suffix.into();
    }

    pub(crate) fn profile(&self) -> &'static str {
        if self.debug { "debug" } else { "release" }
    }

    pub(crate) fn artifact_dir(&self, current_dir: &Path) -> PathBuf {
        current_dir
            .join("target")
            .join(&self.target_triple)
            .join(self.profile())
    }
}
