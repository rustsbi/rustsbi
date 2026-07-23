use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::ValueEnum;

use crate::{Error, Project};

pub const DEFAULT_FIRMWARE_TARGET: &str = "riscv64gc-unknown-none-elf";
pub const DEFAULT_PAYLOAD_TARGET: &str = "riscv64imac-unknown-none-elf";

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
    pub fdt: Option<PathBuf>,
    pub payload: Option<PathBuf>,
    pub jump: bool,
    pub config: Option<PathBuf>,
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
            fdt: None,
            payload: None,
            jump: false,
            config: None,
            test_link_address: None,
            pack: false,
        }
    }
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
    pub target_dir: PathBuf,
    pub config_source: Option<PathBuf>,
    pub config_destination: Option<PathBuf>,
    pub mode_suffix: Option<&'static str>,
    pub pack: bool,
}

impl ImagePlan {
    pub fn resolve(project: &Project, options: BuildOptions) -> Result<Self, Error> {
        if options.payload.is_some() && options.jump {
            return Err(Error::ConflictingBootModes);
        }
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
        let mut environment = BTreeMap::new();
        if let Some(path) = &options.fdt {
            push_feature(&mut features, "fdt");
            environment.insert(
                "PROTOTYPER_FDT_PATH".into(),
                absolute_input(project, path)?.display().to_string(),
            );
        }
        if let Some(path) = &options.payload {
            push_feature(&mut features, "payload");
            environment.insert(
                "PROTOTYPER_PAYLOAD_PATH".into(),
                absolute_input(project, path)?.display().to_string(),
            );
        }
        if options.jump {
            push_feature(&mut features, "jump");
        }
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

        let config_source = matches!(options.role, ImageRole::Firmware | ImageRole::Mtest)
            .then(|| -> Result<PathBuf, Error> {
                let path = options.config.unwrap_or_else(|| {
                    project
                        .root()
                        .join("prototyper/prototyper/config/default.toml")
                });
                absolute_input(project, &path)
            })
            .transpose()?;
        if let Some(path) = &config_source
            && !path.is_file()
        {
            return Err(Error::MissingInput(path.clone()));
        }
        let config_destination = config_source
            .as_ref()
            .map(|_| project.root().join("target/config.toml"));
        let mode_suffix = match options.role {
            ImageRole::Firmware => Some(if options.payload.is_some() {
                "payload"
            } else if options.jump {
                "jump"
            } else {
                "dynamic"
            }),
            _ => None,
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
            config_source,
            config_destination,
            mode_suffix,
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
        (ImageRole::Firmware, _) => "firmware/default.ld",
        (ImageRole::Mtest, _) => "mtest/default.ld",
        (ImageRole::Test, 32) => "test/riscv32.ld",
        (ImageRole::Test, _) => "test/riscv64.ld",
        (ImageRole::Bench, _) => "bench/riscv64.ld",
    };
    project
        .root()
        .join("prototyper/devkit/linker")
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

fn push_feature(features: &mut Vec<String>, feature: &str) {
    if !features.iter().any(|known| known == feature) {
        features.push(feature.into());
    }
}

fn absolute_input(project: &Project, input: &Path) -> Result<PathBuf, Error> {
    let path = if input.is_absolute() {
        input.to_path_buf()
    } else {
        project.root().join(input)
    };
    path.canonicalize()
        .map_err(|error| Error::io("resolve input", error))
}

fn parse_link_address(value: &str) -> Result<usize, Error> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    usize::from_str_radix(digits, 16)
        .ok()
        .filter(|address| address.is_multiple_of(0x1000))
        .ok_or_else(|| Error::InvalidLinkAddress(value.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project::discover(env!("CARGO_MANIFEST_DIR")).unwrap()
    }

    #[test]
    fn default_firmware_plan_preserves_the_current_build_contract() {
        let plan = ExecutionPlan::resolve(&project(), CargoAction::Build, BuildOptions::default())
            .unwrap();
        assert_eq!(plan.image.target, DEFAULT_FIRMWARE_TARGET);
        assert_eq!(plan.image.package, "rustsbi-prototyper");
        assert_eq!(plan.image.mode_suffix, Some("dynamic"));
        assert!(plan.environment["RUSTFLAGS"].contains("relocation-model=pie"));
        assert!(plan.environment["RUSTFLAGS"].contains("firmware/default.ld"));
    }

    #[test]
    fn roles_select_linkers_independently_of_package_names() {
        let project = project();
        for (role, suffix) in [
            (ImageRole::Firmware, "firmware/default.ld"),
            (ImageRole::Mtest, "mtest/default.ld"),
            (ImageRole::Test, "test/riscv64.ld"),
            (ImageRole::Bench, "bench/riscv64.ld"),
        ] {
            let plan = ImagePlan::resolve(
                &project,
                BuildOptions {
                    role,
                    ..BuildOptions::default()
                },
            )
            .unwrap();
            assert!(plan.linker.ends_with(suffix));
        }
    }

    #[test]
    fn rejects_conflicting_boot_modes_and_misaligned_test_addresses() {
        assert!(matches!(
            ImagePlan::resolve(
                &project(),
                BuildOptions {
                    payload: Some(PathBuf::from("Cargo.toml")),
                    jump: true,
                    ..BuildOptions::default()
                }
            ),
            Err(Error::ConflictingBootModes)
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
            [("__mtest_descriptor_size".into(), "12".into())]
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
