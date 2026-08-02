use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Deserialize;

use crate::{Error, Project};

/// Standard OpenSBI firmware type selected for one Prototyper image.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, ValueEnum)]
pub enum FirmwareType {
    /// The previous stage supplies an `fw_dynamic_info` structure in `a2`.
    #[serde(rename = "FW_DYNAMIC")]
    #[value(name = "FW_DYNAMIC")]
    Dynamic,
    /// The next stage is loaded separately at a fixed address.
    #[serde(rename = "FW_JUMP")]
    #[value(name = "FW_JUMP")]
    Jump,
    /// The next-stage binary is linked into the firmware image.
    #[serde(rename = "FW_PAYLOAD")]
    #[value(name = "FW_PAYLOAD")]
    Payload,
}

impl FirmwareType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dynamic => "FW_DYNAMIC",
            Self::Jump => "FW_JUMP",
            Self::Payload => "FW_PAYLOAD",
        }
    }

    pub(crate) const fn contract_kind(self) -> u8 {
        match self {
            Self::Dynamic => 1,
            Self::Jump => 2,
            Self::Payload => 3,
        }
    }
}

/// RISC-V privilege mode used when entering a fixed next stage.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NextMode {
    User,
    #[default]
    Supervisor,
    Machine,
}

impl NextMode {
    pub(crate) const fn encoding(self) -> u8 {
        match self {
            Self::User => 0,
            Self::Supervisor => 1,
            Self::Machine => 3,
        }
    }
}

/// Human-authored firmware product configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct FirmwareManifest {
    pub platform: String,
    pub firmware_type: FirmwareType,
    pub device_tree: Option<PathBuf>,
    pub next_stage: Option<NextStageManifest>,
}

/// Next-stage source used by `FW_JUMP` or `FW_PAYLOAD`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct NextStageManifest {
    pub address: Option<u64>,
    pub binary: Option<PathBuf>,
    pub package: Option<String>,
    #[serde(default)]
    pub mode: NextMode,
}

/// Fully validated paths and values consumed by the image planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareConfig {
    pub source: PathBuf,
    pub platform: String,
    pub firmware_type: FirmwareType,
    pub device_tree: Option<PathBuf>,
    pub next_stage: NextStageSource,
}

/// Validated next-stage source with mutually exclusive variants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NextStageSource {
    Dynamic,
    Jump { address: u64, mode: NextMode },
    PayloadBinary { binary: PathBuf, mode: NextMode },
    PayloadPackage { package: String, mode: NextMode },
}

impl FirmwareConfig {
    /// Loads the canonical project file or an explicit configuration override.
    pub fn load(project: &Project, explicit: Option<&Path>) -> Result<Self, Error> {
        let source = explicit
            .map(Path::to_path_buf)
            .unwrap_or_else(|| project.root().join("prototyper/firmware/Prototyper.toml"));
        let source = absolute_existing(project, &source)?;
        let contents = fs::read_to_string(&source)
            .map_err(|error| Error::io("read Prototyper.toml", error))?;
        let manifest: FirmwareManifest =
            toml::from_str(&contents).map_err(|error| Error::InvalidManifest(error.to_string()))?;
        Self::validate(project, source, manifest)
    }

    fn validate(
        project: &Project,
        source: PathBuf,
        manifest: FirmwareManifest,
    ) -> Result<Self, Error> {
        if manifest.platform != "qemu-virt" {
            return Err(Error::UnsupportedPlatform(manifest.platform));
        }
        let device_tree = manifest
            .device_tree
            .as_deref()
            .map(|path| absolute_existing(project, path))
            .transpose()?;
        let next_stage = match (manifest.firmware_type, manifest.next_stage) {
            (FirmwareType::Dynamic, None) => NextStageSource::Dynamic,
            (FirmwareType::Dynamic, Some(_)) => {
                return Err(Error::InvalidFirmwareContract(
                    "FW_DYNAMIC does not accept a build-time next_stage",
                ));
            }
            (FirmwareType::Jump, Some(stage))
                if stage.address.is_some() && stage.binary.is_none() && stage.package.is_none() =>
            {
                let address = stage.address.expect("guarded above");
                if address == 0 || address & 1 != 0 {
                    return Err(Error::InvalidFirmwareContract(
                        "FW_JUMP next_stage.address must be non-zero and 2-byte aligned",
                    ));
                }
                NextStageSource::Jump {
                    address,
                    mode: stage.mode,
                }
            }
            (FirmwareType::Payload, Some(stage))
                if stage.binary.is_some() && stage.package.is_none() && stage.address.is_none() =>
            {
                NextStageSource::PayloadBinary {
                    binary: absolute_existing(
                        project,
                        stage.binary.as_deref().expect("guarded above"),
                    )?,
                    mode: stage.mode,
                }
            }
            (FirmwareType::Payload, Some(stage))
                if stage.package.is_some() && stage.binary.is_none() && stage.address.is_none() =>
            {
                NextStageSource::PayloadPackage {
                    package: stage.package.expect("guarded above"),
                    mode: stage.mode,
                }
            }
            (FirmwareType::Jump, _) => {
                return Err(Error::InvalidFirmwareContract(
                    "FW_JUMP requires exactly next_stage.address",
                ));
            }
            (FirmwareType::Payload, _) => {
                return Err(Error::InvalidFirmwareContract(
                    "FW_PAYLOAD requires exactly next_stage.binary or next_stage.package",
                ));
            }
        };
        Ok(Self {
            source,
            platform: manifest.platform,
            firmware_type: manifest.firmware_type,
            device_tree,
            next_stage,
        })
    }

    /// Replaces a package payload with the binary produced by devkit.
    pub fn with_payload_binary(mut self, binary: PathBuf) -> Self {
        let mode = match self.next_stage {
            NextStageSource::PayloadPackage { mode, .. } => mode,
            _ => return self,
        };
        self.next_stage = NextStageSource::PayloadBinary { binary, mode };
        self
    }
}

fn absolute_existing(project: &Project, input: &Path) -> Result<PathBuf, Error> {
    let path = if input.is_absolute() {
        input.to_path_buf()
    } else {
        project.root().join(input)
    };
    path.canonicalize().map_err(|_| Error::MissingInput(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project::discover(env!("CARGO_MANIFEST_DIR")).unwrap()
    }

    #[test]
    fn canonical_manifest_uses_standard_firmware_names() {
        let config = FirmwareConfig::load(&project(), None).unwrap();
        assert_eq!(config.platform, "qemu-virt");
        assert_eq!(config.firmware_type, FirmwareType::Payload);
        assert!(matches!(
            config.next_stage,
            NextStageSource::PayloadPackage { .. }
        ));
    }

    #[test]
    fn firmware_type_names_are_not_cargo_feature_names() {
        assert_eq!(FirmwareType::Dynamic.name(), "FW_DYNAMIC");
        assert_eq!(FirmwareType::Jump.name(), "FW_JUMP");
        assert_eq!(FirmwareType::Payload.name(), "FW_PAYLOAD");
    }
}
