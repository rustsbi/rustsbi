use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use cargo_prototyper::{
    Architecture, BuildOptions, CargoAction, ExecutionPlan, FirmwareConfig, FirmwareType,
    ImageRole, LaunchPlan, NextStageSource, Project, execute, run_launch,
};
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    bin_name = "cargo prototyper",
    about = "Build and run RustSBI Prototyper images"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Compatibility form: options without a subcommand imply `build`.
    #[command(flatten)]
    legacy_build: ImageArgs,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build one bootable image and its final artifacts.
    Build(ImageArgs),
    /// Check one target image without promising a bootable artifact.
    Check(ImageArgs),
    /// Run Clippy for one target image.
    Clippy(ImageArgs),
    /// Build and launch one firmware image on QEMU virt.
    Run(RunArgs),
    /// Run the S-mode suite and every isolated M-mode case under QEMU.
    Test(TestArgs),
}

#[derive(Clone, Debug, Args)]
struct ImageArgs {
    /// Select the image/linker role independently of its temporary package.
    #[arg(long, value_enum, default_value_t = ImageRole::Firmware)]
    image: ImageRole,

    /// Select RV32 or RV64 as one coherent build and QEMU profile.
    #[arg(long, value_enum, default_value_t)]
    arch: Architecture,

    #[arg(long, short = 'f', value_delimiter = ',')]
    features: Vec<String>,

    #[arg(long)]
    debug: bool,

    /// Use an explicit project file instead of the package's Prototyper.toml.
    #[arg(long, short = 'c')]
    config: Option<PathBuf>,

    /// Override the standard firmware type for this invocation.
    #[arg(long, value_enum)]
    firmware_type: Option<FirmwareType>,

    /// Override the DTB linked into the firmware.
    #[arg(long)]
    device_tree: Option<PathBuf>,

    /// Override the binary linked by FW_PAYLOAD.
    #[arg(long)]
    next_stage_binary: Option<PathBuf>,

    /// Override the fixed hexadecimal address used by FW_JUMP.
    #[arg(long)]
    next_stage_address: Option<String>,

    #[arg(long, env = "RUSTSBI_TEST_LINK_ADDRESS")]
    test_link_address: Option<String>,

    /// Package the S-mode test or benchmark with an existing firmware binary.
    #[arg(long)]
    pack: bool,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[command(flatten)]
    image: ImageArgs,

    /// Kill and classify QEMU after this many seconds.
    #[arg(long, default_value_t = 10)]
    timeout: u64,
}

#[derive(Debug, Args)]
struct TestArgs {
    /// Select the RISC-V architecture tested by this invocation.
    #[arg(long, value_enum, default_value_t)]
    arch: Architecture,

    /// Kill and classify each QEMU invocation after this many seconds.
    #[arg(long, default_value_t = 60)]
    timeout: u64,
}

impl ImageArgs {
    fn build_options(&self) -> BuildOptions {
        BuildOptions {
            role: self.image,
            target: Some(self.arch.target(self.image).into()),
            release: !self.debug,
            features: self.features.clone(),
            firmware: None,
            test_link_address: self.test_link_address.clone(),
            pack: self.pack,
        }
    }
}

fn main() -> ExitCode {
    match try_main() {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("cargo prototyper: {error}");
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> Result<i32, cargo_prototyper::Error> {
    let cli = Cli::parse();
    let project = Project::discover(
        std::env::current_dir()
            .map_err(|error| cargo_prototyper::Error::io("read current directory", error))?,
    )?;
    match cli.command {
        None => build(&project, CargoAction::Build, cli.legacy_build).map(|_| 0),
        Some(Command::Build(arguments)) => {
            build(&project, CargoAction::Build, arguments).map(|_| 0)
        }
        Some(Command::Check(arguments)) => {
            build(&project, CargoAction::Check, arguments).map(|_| 0)
        }
        Some(Command::Clippy(arguments)) => {
            build(&project, CargoAction::Clippy, arguments).map(|_| 0)
        }
        Some(Command::Run(arguments)) => {
            let options = resolved_options(&project, CargoAction::Build, &arguments.image)?;
            let (plan, artifacts) = execute_options(&project, CargoAction::Build, options)?;
            let plan = LaunchPlan::qemu_virt(
                project.root().to_path_buf(),
                &plan.image,
                &artifacts,
                &cargo_prototyper::QemuConfig::default(),
                Duration::from_secs(arguments.timeout),
            )?;
            let outcome = run_launch(&plan)?;
            match &outcome {
                cargo_prototyper::Outcome::Exited { stdout, stderr, .. }
                | cargo_prototyper::Outcome::TimedOut { stdout, stderr } => {
                    print!("{}", String::from_utf8_lossy(stdout));
                    eprint!("{}", String::from_utf8_lossy(stderr));
                }
            }
            Ok(outcome.exit_code())
        }
        Some(Command::Test(arguments)) => test(&project, arguments),
    }
}

fn test(project: &Project, arguments: TestArgs) -> Result<i32, cargo_prototyper::Error> {
    let timeout = Duration::from_secs(arguments.timeout);
    let stage = execute_options(
        project,
        CargoAction::Build,
        BuildOptions {
            role: ImageRole::Test,
            target: Some(arguments.arch.target(ImageRole::Test).into()),
            ..BuildOptions::default()
        },
    )?
    .1;
    let firmware = BuildOptions {
        target: Some(arguments.arch.target(ImageRole::Firmware).into()),
        firmware: Some(FirmwareConfig {
            source: PathBuf::from("<devkit:test>"),
            platform: "qemu-virt".into(),
            firmware_type: FirmwareType::Payload,
            device_tree: None,
            next_stage: NextStageSource::PayloadBinary {
                binary: stage.binary,
                mode: cargo_prototyper::NextMode::Supervisor,
            },
        }),
        ..BuildOptions::default()
    };
    let s_mode = build_and_launch(project, firmware, timeout, &[])?;
    print_outcome(&s_mode);
    if !s_mode.success() {
        return Ok(s_mode.exit_code());
    }

    let listing = build_and_launch(
        project,
        BuildOptions {
            role: ImageRole::Mtest,
            target: Some(arguments.arch.target(ImageRole::Mtest).into()),
            ..BuildOptions::default()
        },
        timeout,
        &[("RUSTSBI_MTEST_LIST", "1")],
    )?;
    print_outcome(&listing);
    if !listing.success() {
        return Ok(listing.exit_code());
    }
    let names = mtest_names(&listing);
    if names.is_empty() {
        eprintln!("cargo prototyper: M-mode registry published no cases");
        return Ok(1);
    }

    for name in names {
        eprintln!("cargo prototyper: running M-mode case {name}");
        let outcome = build_and_launch(
            project,
            BuildOptions {
                role: ImageRole::Mtest,
                target: Some(arguments.arch.target(ImageRole::Mtest).into()),
                ..BuildOptions::default()
            },
            timeout,
            &[("RUSTSBI_MTEST_FILTER", name)],
        )?;
        print_outcome(&outcome);
        if !outcome.success() {
            return Ok(outcome.exit_code());
        }
    }
    Ok(0)
}

fn build_and_launch(
    project: &Project,
    options: BuildOptions,
    timeout: Duration,
    environment: &[(&str, &str)],
) -> Result<cargo_prototyper::Outcome, cargo_prototyper::Error> {
    let (plan, artifacts) =
        execute_options_with_environment(project, CargoAction::Build, options, environment)?;
    let launch = LaunchPlan::qemu_virt(
        project.root().to_path_buf(),
        &plan.image,
        &artifacts,
        &cargo_prototyper::QemuConfig::default(),
        timeout,
    )?;
    run_launch(&launch)
}

fn print_outcome(outcome: &cargo_prototyper::Outcome) {
    match outcome {
        cargo_prototyper::Outcome::Exited { stdout, stderr, .. }
        | cargo_prototyper::Outcome::TimedOut { stdout, stderr } => {
            print!("{}", String::from_utf8_lossy(stdout));
            eprint!("{}", String::from_utf8_lossy(stderr));
        }
    }
}

fn mtest_names(outcome: &cargo_prototyper::Outcome) -> Vec<&str> {
    const PREFIX: &str = "@@RUSTSBI_MTEST type=CASE name=";
    let stdout = match outcome {
        cargo_prototyper::Outcome::Exited { stdout, .. }
        | cargo_prototyper::Outcome::TimedOut { stdout, .. } => stdout,
    };
    std::str::from_utf8(stdout)
        .unwrap_or("")
        .lines()
        .filter_map(|line| line.strip_prefix(PREFIX))
        .collect()
}

fn build(
    project: &Project,
    action: CargoAction,
    arguments: ImageArgs,
) -> Result<cargo_prototyper::ArtifactSet, cargo_prototyper::Error> {
    let options = resolved_options(project, action, &arguments)?;
    execute_options(project, action, options).map(|(_, artifacts)| artifacts)
}

fn resolved_options(
    project: &Project,
    _action: CargoAction,
    arguments: &ImageArgs,
) -> Result<BuildOptions, cargo_prototyper::Error> {
    let mut options = arguments.build_options();
    if arguments.image != ImageRole::Firmware {
        return Ok(options);
    }
    let mut config = FirmwareConfig::load(project, arguments.config.as_deref())?;
    let mode = match &config.next_stage {
        NextStageSource::Jump { mode, .. }
        | NextStageSource::PayloadBinary { mode, .. }
        | NextStageSource::PayloadPackage { mode, .. } => *mode,
        NextStageSource::Dynamic => cargo_prototyper::NextMode::Supervisor,
    };
    let firmware_type = arguments.firmware_type.unwrap_or(config.firmware_type);
    if let Some(path) = &arguments.device_tree {
        config.device_tree = Some(project.input(path)?);
    }
    config.firmware_type = firmware_type;
    config.next_stage = match firmware_type {
        FirmwareType::Dynamic => {
            if arguments.next_stage_binary.is_some() || arguments.next_stage_address.is_some() {
                return Err(cargo_prototyper::Error::InvalidFirmwareContract(
                    "FW_DYNAMIC does not accept next-stage overrides",
                ));
            }
            NextStageSource::Dynamic
        }
        FirmwareType::Jump => {
            let address = if let Some(value) = &arguments.next_stage_address {
                parse_address(value)?
            } else if let NextStageSource::Jump { address, .. } = config.next_stage {
                address
            } else {
                return Err(cargo_prototyper::Error::InvalidFirmwareContract(
                    "FW_JUMP requires --next-stage-address or next_stage.address",
                ));
            };
            NextStageSource::Jump { address, mode }
        }
        FirmwareType::Payload => {
            if let Some(path) = &arguments.next_stage_binary {
                NextStageSource::PayloadBinary {
                    binary: project.input(path)?,
                    mode,
                }
            } else {
                match config.next_stage {
                    NextStageSource::PayloadBinary { binary, .. } => {
                        NextStageSource::PayloadBinary { binary, mode }
                    }
                    NextStageSource::PayloadPackage { package, .. } => {
                        let expected = "rustsbi-test-kernel";
                        if package != expected {
                            return Err(cargo_prototyper::Error::InvalidFirmwareContract(
                                "only the rustsbi-test-kernel package is currently supported",
                            ));
                        }
                        let stage = execute_options(
                            project,
                            CargoAction::Build,
                            BuildOptions {
                                role: ImageRole::Test,
                                target: Some(arguments.arch.target(ImageRole::Test).into()),
                                release: !arguments.debug,
                                test_link_address: arguments.test_link_address.clone(),
                                ..BuildOptions::default()
                            },
                        )?
                        .1;
                        NextStageSource::PayloadBinary {
                            binary: stage.binary,
                            mode,
                        }
                    }
                    _ => {
                        return Err(cargo_prototyper::Error::InvalidFirmwareContract(
                            "FW_PAYLOAD requires --next-stage-binary or a payload source",
                        ));
                    }
                }
            }
        }
    };
    options.firmware = Some(config);
    Ok(options)
}

fn parse_address(value: &str) -> Result<u64, cargo_prototyper::Error> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(digits, 16)
        .ok()
        .filter(|address| *address != 0 && address & 1 == 0)
        .ok_or(cargo_prototyper::Error::InvalidFirmwareContract(
            "next-stage address must be non-zero aligned hexadecimal",
        ))
}

fn execute_options(
    project: &Project,
    action: CargoAction,
    options: BuildOptions,
) -> Result<(ExecutionPlan, cargo_prototyper::ArtifactSet), cargo_prototyper::Error> {
    execute_options_with_environment(project, action, options, &[])
}

fn execute_options_with_environment(
    project: &Project,
    action: CargoAction,
    options: BuildOptions,
    additional_environment: &[(&str, &str)],
) -> Result<(ExecutionPlan, cargo_prototyper::ArtifactSet), cargo_prototyper::Error> {
    let mut plan = ExecutionPlan::resolve(project, action, options)?;
    plan.environment.extend(
        additional_environment
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).into())),
    );
    eprintln!(
        "cargo prototyper: {} {} for {}",
        match action {
            CargoAction::Build => "building",
            CargoAction::Check => "checking",
            CargoAction::Clippy => "linting",
        },
        plan.image.role.name(),
        plan.image.target
    );
    let artifacts = execute(&plan)?;
    if action == CargoAction::Build {
        eprintln!(
            "cargo prototyper: binary {}",
            artifacts.named_binary.display()
        );
    }
    Ok((plan, artifacts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_stage_addresses_require_hexadecimal_alignment() {
        assert_eq!(parse_address("0x80200000").unwrap(), 0x8020_0000);
        assert!(parse_address("0x80200001").is_err());
    }
}
