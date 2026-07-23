use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::ValueEnum;

use crate::{Error, FirmwareConfig, FirmwareType, NextMode, NextStageSource, Project};

pub const DEFAULT_FIRMWARE_TARGET: &str = "riscv64imac-unknown-none-elf";
pub const DEFAULT_PAYLOAD_TARGET: &str = "riscv64imac-unknown-none-elf";
pub const RV32_FIRMWARE_TARGET: &str = "riscv32imac-unknown-none-elf";
pub const RV32_PAYLOAD_TARGET: &str = "riscv32imac-unknown-none-elf";

/// RISC-V architecture selected for one devkit invocation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum Architecture {
    Rv32,
    #[default]
    Rv64,
}

impl Architecture {
    /// Returns the fixed Rust target for this architecture and image role.
    pub const fn target(self, role: ImageRole) -> &'static str {
        match (self, role) {
            (Self::Rv32, ImageRole::Firmware | ImageRole::Mtest) => RV32_FIRMWARE_TARGET,
            (Self::Rv32, ImageRole::Test) => RV32_PAYLOAD_TARGET,
            (Self::Rv32, ImageRole::Bench) => RV32_PAYLOAD_TARGET,
            (Self::Rv64, ImageRole::Firmware | ImageRole::Mtest) => DEFAULT_FIRMWARE_TARGET,
            (Self::Rv64, ImageRole::Test | ImageRole::Bench) => DEFAULT_PAYLOAD_TARGET,
        }
    }
}

/// A bootable image role with an independently reviewed linker contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ImageRole {
    Firmware,
    Mtest,
    Test,
    Bench,
}

impl ImageRole {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Firmware => "firmware",
            Self::Mtest => "mtest",
            Self::Test => "test",
            Self::Bench => "bench",
        }
    }

    const fn package(self) -> &'static str {
        match self {
            Self::Firmware | Self::Mtest => "rustsbi-prototyper",
            Self::Test => "rustsbi-test-kernel",
            Self::Bench => "rustsbi-bench-kernel",
        }
    }

    const fn binary(self) -> &'static str {
        match self {
            Self::Firmware => "rustsbi-prototyper",
            Self::Mtest => "rustsbi-prototyper-mtest",
            Self::Test => "rustsbi-test-kernel",
            Self::Bench => "rustsbi-bench-kernel",
        }
    }
}

/// Cargo operation selected after CLI and project configuration resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CargoAction {
    Build,
    Check,
    Clippy,
}

impl CargoAction {
    const fn command(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Check => "check",
            Self::Clippy => "clippy",
        }
    }
}

/// User/project inputs resolved into one immutable image plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildOptions {
    pub role: ImageRole,
    pub target: Option<String>,
    pub release: bool,
    pub features: Vec<String>,
    pub firmware: Option<FirmwareConfig>,
    pub test_link_address: Option<String>,
    pub pack: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            role: ImageRole::Firmware,
            target: None,
            release: true,
            features: Vec::new(),
            firmware: None,
            test_link_address: None,
            pack: false,
        }
    }
}

/// One binary input converted to a RISC-V relocatable object before linking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkInput {
    pub section: &'static str,
    pub object: PathBuf,
    pub contents: LinkInputContents,
}

/// Source of bytes for a generated link input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkInputContents {
    Bytes(Vec<u8>),
    File(PathBuf),
}

/// All inputs that determine one Cargo invocation and its final image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImagePlan {
    pub role: ImageRole,
    pub target: String,
    pub release: bool,
    pub package: &'static str,
    pub binary: &'static str,
    pub features: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub linker: PathBuf,
    pub linker_symbols: Vec<(String, String)>,
    pub link_inputs: Vec<LinkInput>,
    pub target_dir: PathBuf,
    pub mode_suffix: Option<&'static str>,
    pub firmware_type: Option<FirmwareType>,
    pub pack: bool,
}

impl ImagePlan {
    pub fn resolve(project: &Project, options: BuildOptions) -> Result<Self, Error> {
        if options.pack && !matches!(options.role, ImageRole::Test | ImageRole::Bench) {
            return Err(Error::UnsupportedPackaging(options.role.name()));
        }

        let target = options.target.unwrap_or_else(|| match options.role {
            ImageRole::Firmware | ImageRole::Mtest => DEFAULT_FIRMWARE_TARGET.to_owned(),
            ImageRole::Test | ImageRole::Bench => DEFAULT_PAYLOAD_TARGET.to_owned(),
        });
        let width = target_width(&target)?;
        if options.role == ImageRole::Bench && width != 64 {
            return Err(Error::UnsupportedRoleTarget {
                role: options.role.name(),
                target,
            });
        }

        let mut features = options.features;
        let environment = BTreeMap::new();
        if options.role == ImageRole::Mtest {
            push_feature(&mut features, "mtest");
        }
        features.sort();
        features.dedup();

        let linker = linker_path(project, options.role, width);
        if !linker.is_file() {
            return Err(Error::MissingInput(linker));
        }
        let mut linker_symbols = Vec::new();
        if matches!(options.role, ImageRole::Firmware | ImageRole::Mtest) {
            linker_symbols.push((
                "__prototyper_payload_address".into(),
                format!("0x{:x}", payload_address(width)),
            ));
        }
        if options.role == ImageRole::Mtest {
            linker_symbols.push((
                "__mtest_descriptor_size".into(),
                if width == 32 { "12" } else { "24" }.into(),
            ));
        }
        if let Some(value) = options.test_link_address {
            if options.role != ImageRole::Test {
                return Err(Error::InvalidLinkAddress(value));
            }
            let address = parse_link_address(&value)?;
            linker_symbols.push((
                "__rustsbi_test_link_address".into(),
                format!("0x{address:x}"),
            ));
        }

        let firmware = match options.role {
            ImageRole::Firmware => Some(options.firmware.ok_or(Error::InvalidFirmwareContract(
                "firmware configuration was not resolved",
            ))?),
            ImageRole::Mtest => Some(FirmwareConfig {
                source: PathBuf::from("<devkit:mtest>"),
                platform: "qemu-virt".into(),
                firmware_type: FirmwareType::Jump,
                device_tree: None,
                next_stage: NextStageSource::Jump {
                    address: 0x8020_0000,
                    mode: NextMode::Supervisor,
                },
            }),
            ImageRole::Test | ImageRole::Bench => None,
        };
        let link_directory = project
            .root()
            .join("target/prototyper-link")
            .join(&target)
            .join(options.role.name());
        let mut link_inputs = Vec::new();
        let (firmware_type, mode_suffix) = if let Some(firmware) = firmware {
            if firmware.platform != "qemu-virt" {
                return Err(Error::UnsupportedPlatform(firmware.platform));
            }
            let (address, mode) = match &firmware.next_stage {
                NextStageSource::Dynamic => (0, NextMode::Supervisor),
                NextStageSource::Jump { address, mode } => (*address, *mode),
                NextStageSource::PayloadBinary { binary, mode } => {
                    link_inputs.push(LinkInput {
                        section: ".prototyper.payload",
                        object: link_directory.join("payload.o"),
                        contents: LinkInputContents::File(binary.clone()),
                    });
                    (payload_address(width), *mode)
                }
                NextStageSource::PayloadPackage { .. } => {
                    return Err(Error::InvalidFirmwareContract(
                        "the FW_PAYLOAD package must be built before image planning",
                    ));
                }
            };
            link_inputs.push(LinkInput {
                section: ".prototyper.contract",
                object: link_directory.join("contract.o"),
                contents: LinkInputContents::Bytes(encode_contract(
                    firmware.firmware_type,
                    mode,
                    address,
                )),
            });
            if let Some(device_tree) = firmware.device_tree {
                link_inputs.push(LinkInput {
                    section: ".prototyper.dtb",
                    object: link_directory.join("device-tree.o"),
                    contents: LinkInputContents::File(device_tree),
                });
            }
            (
                Some(firmware.firmware_type),
                Some(match firmware.firmware_type {
                    FirmwareType::Dynamic => "fw-dynamic",
                    FirmwareType::Jump => "fw-jump",
                    FirmwareType::Payload => "fw-payload",
                }),
            )
        } else {
            (None, None)
        };

        Ok(Self {
            role: options.role,
            target_dir: project.target_dir(&target, options.release),
            target,
            release: options.release,
            package: options.role.package(),
            binary: options.role.binary(),
            features,
            environment,
            linker,
            linker_symbols,
            link_inputs,
            mode_suffix,
            firmware_type,
            pack: options.pack,
        })
    }

    pub fn rustflags(&self) -> String {
        let mut flags = Vec::<String>::new();
        if matches!(self.role, ImageRole::Firmware | ImageRole::Mtest) {
            flags.extend(["-C", "relocation-model=pie", "-C", "link-arg=-pie"].map(str::to_owned));
        }
        for (name, value) in &self.linker_symbols {
            flags.extend(["-C".into(), format!("link-arg=--defsym={name}={value}")]);
        }
        for input in &self.link_inputs {
            flags.extend(["-C".into(), format!("link-arg={}", input.object.display())]);
        }
        // Symbols used while the script advances `.` must be defined before
        // the script is evaluated. In particular, the test image's optional
        // load address cannot be retroactively changed by a later `--defsym`.
        flags.extend(["-C".into(), format!("link-arg=-T{}", self.linker.display())]);
        flags.join(" ")
    }
}

/// A fully resolved Cargo process. No later stage reinterprets image inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPlan {
    pub project_root: PathBuf,
    pub action: CargoAction,
    pub image: ImagePlan,
    pub program: String,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
}

impl ExecutionPlan {
    pub fn resolve(
        project: &Project,
        action: CargoAction,
        options: BuildOptions,
    ) -> Result<Self, Error> {
        let image = ImagePlan::resolve(project, options)?;
        let mut arguments = vec![
            action.command().into(),
            "--package".into(),
            image.package.into(),
            "--bin".into(),
            image.binary.into(),
            "--target".into(),
            image.target.clone(),
            "--no-default-features".into(),
            "-Z".into(),
            "build-std=core,alloc".into(),
        ];
        if image.release {
            arguments.push("--release".into());
        }
        if !image.features.is_empty() {
            arguments.extend(["--features".into(), image.features.join(",")]);
        }
        if action == CargoAction::Clippy {
            arguments.extend(["--".into(), "-D".into(), "warnings".into()]);
        }

        let mut environment = image.environment.clone();
        environment.insert("RUSTFLAGS".into(), image.rustflags());
        Ok(Self {
            project_root: project.root().to_path_buf(),
            action,
            image,
            program: "cargo".into(),
            arguments,
            environment,
        })
    }
}

fn linker_path(project: &Project, role: ImageRole, width: u8) -> PathBuf {
    let relative = match (role, width) {
        (ImageRole::Firmware, _) => "firmware.ld",
        (ImageRole::Mtest, _) => "mtest.ld",
        (ImageRole::Test, 32) => "test-rv32.ld",
        (ImageRole::Test, _) => "test-rv64.ld",
        (ImageRole::Bench, _) => "bench-rv64.ld",
    };
    project
        .root()
        .join("prototyper/devkit/platforms/qemu-virt")
        .join(relative)
}

fn target_width(target: &str) -> Result<u8, Error> {
    if target.contains("riscv32") {
        Ok(32)
    } else if target.contains("riscv64") {
        Ok(64)
    } else {
        Err(Error::UnsupportedTarget(target.into()))
    }
}

const fn payload_address(width: u8) -> u64 {
    if width == 32 {
        0x8040_0000
    } else {
        0x8020_0000
    }
}

fn push_feature(features: &mut Vec<String>, feature: &str) {
    if !features.iter().any(|known| known == feature) {
        features.push(feature.into());
    }
}

fn parse_link_address(value: &str) -> Result<usize, Error> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    usize::from_str_radix(digits, 16)
        .ok()
        .filter(|address| address.is_multiple_of(0x1000))
        .ok_or_else(|| Error::InvalidLinkAddress(value.into()))
}

fn encode_contract(firmware_type: FirmwareType, mode: NextMode, next_address: u64) -> Vec<u8> {
    const MAGIC: u32 = 0x5054_5950;
    const VERSION: u16 = 1;
    let mut bytes = Vec::with_capacity(40);
    bytes.extend_from_slice(&MAGIC.to_le_bytes());
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.push(firmware_type.contract_kind());
    bytes.push(mode.encoding());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&next_address.to_le_bytes());
    bytes.extend_from_slice(&0x8000_0000u64.to_le_bytes());
    bytes.extend_from_slice(&0x9000_0000u64.to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project::discover(env!("CARGO_MANIFEST_DIR")).unwrap()
    }

    fn dynamic_config() -> FirmwareConfig {
        FirmwareConfig {
            source: PathBuf::from("<test>"),
            platform: "qemu-virt".into(),
            firmware_type: FirmwareType::Dynamic,
            device_tree: None,
            next_stage: NextStageSource::Dynamic,
        }
    }

    #[test]
    fn firmware_plan_uses_the_normalized_link_contract() {
        let plan = ExecutionPlan::resolve(
            &project(),
            CargoAction::Build,
            BuildOptions {
                firmware: Some(dynamic_config()),
                ..BuildOptions::default()
            },
        )
        .unwrap();
        assert_eq!(plan.image.target, DEFAULT_FIRMWARE_TARGET);
        assert_eq!(plan.image.package, "rustsbi-prototyper");
        assert_eq!(plan.image.mode_suffix, Some("fw-dynamic"));
        assert_eq!(plan.image.link_inputs.len(), 1);
        assert!(plan.environment["RUSTFLAGS"].contains("relocation-model=pie"));
        assert!(plan.environment["RUSTFLAGS"].contains("qemu-virt/firmware.ld"));
    }

    #[test]
    fn roles_select_linkers_independently_of_package_names() {
        let project = project();
        for (role, suffix) in [
            (ImageRole::Firmware, "firmware.ld"),
            (ImageRole::Mtest, "mtest.ld"),
            (ImageRole::Test, "test-rv64.ld"),
            (ImageRole::Bench, "bench-rv64.ld"),
        ] {
            let plan = ImagePlan::resolve(
                &project,
                BuildOptions {
                    role,
                    firmware: (role == ImageRole::Firmware).then(dynamic_config),
                    ..BuildOptions::default()
                },
            )
            .unwrap();
            assert!(plan.linker.ends_with(suffix));
        }
    }

    #[test]
    fn rejects_missing_firmware_contract_and_misaligned_test_addresses() {
        assert!(matches!(
            ImagePlan::resolve(&project(), BuildOptions::default()),
            Err(Error::InvalidFirmwareContract(_))
        ));
        assert!(matches!(
            ImagePlan::resolve(
                &project(),
                BuildOptions {
                    role: ImageRole::Test,
                    test_link_address: Some("0x80200001".into()),
                    ..BuildOptions::default()
                }
            ),
            Err(Error::InvalidLinkAddress(_))
        ));
        assert!(matches!(
            ImagePlan::resolve(
                &project(),
                BuildOptions {
                    firmware: Some(dynamic_config()),
                    pack: true,
                    ..BuildOptions::default()
                }
            ),
            Err(Error::UnsupportedPackaging("firmware"))
        ));
    }

    #[test]
    fn mtest_descriptor_size_follows_xlen() {
        let plan = ImagePlan::resolve(
            &project(),
            BuildOptions {
                role: ImageRole::Mtest,
                target: Some("riscv32imac-unknown-none-elf".into()),
                ..BuildOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            plan.linker_symbols,
            [
                ("__prototyper_payload_address".into(), "0x80400000".into()),
                ("__mtest_descriptor_size".into(), "12".into()),
            ]
        );
    }

    #[test]
    fn linker_symbols_are_defined_before_the_script_is_evaluated() {
        let plan = ImagePlan::resolve(
            &project(),
            BuildOptions {
                role: ImageRole::Test,
                test_link_address: Some("0x80300000".into()),
                ..BuildOptions::default()
            },
        )
        .unwrap();
        let flags = plan.rustflags();
        let symbol = flags.find("--defsym=__rustsbi_test_link_address").unwrap();
        let script = flags.find("link-arg=-T").unwrap();
        assert!(symbol < script);
    }
}
