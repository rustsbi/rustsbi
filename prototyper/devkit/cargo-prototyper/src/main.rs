use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use cargo_prototyper::{
    BuildOptions, CargoAction, ExecutionPlan, ImageRole, LaunchPlan, Project, execute, run_launch,
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
}

#[derive(Clone, Debug, Args)]
struct ImageArgs {
    /// Select the image/linker role independently of its temporary package.
    #[arg(long, value_enum, default_value_t = ImageRole::Firmware)]
    image: ImageRole,

    #[arg(long)]
    target: Option<String>,

    #[arg(long, short = 'f', value_delimiter = ',')]
    features: Vec<String>,

    #[arg(long, env = "PROTOTYPER_FDT_PATH")]
    fdt: Option<PathBuf>,

    #[arg(long, env = "PROTOTYPER_PAYLOAD_PATH")]
    payload: Option<PathBuf>,

    #[arg(long)]
    jump: bool,

    #[arg(long)]
    debug: bool,

    #[arg(long, short = 'c')]
    config_file: Option<PathBuf>,

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

impl From<ImageArgs> for BuildOptions {
    fn from(arguments: ImageArgs) -> Self {
        Self {
            role: arguments.image,
            target: arguments.target,
            release: !arguments.debug,
            features: arguments.features,
            fdt: arguments.fdt,
            payload: arguments.payload,
            jump: arguments.jump,
            config: arguments.config_file,
            test_link_address: arguments.test_link_address,
            pack: arguments.pack,
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
            let mut options: BuildOptions = arguments.image.into();
            if options.role == ImageRole::Firmware && options.payload.is_none() && !options.jump {
                let payload = execute_options(
                    &project,
                    CargoAction::Build,
                    default_payload_options(&options),
                )?
                .1;
                options.payload = Some(payload.binary);
            }
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
    }
}

fn build(
    project: &Project,
    action: CargoAction,
    arguments: ImageArgs,
) -> Result<cargo_prototyper::ArtifactSet, cargo_prototyper::Error> {
    execute_options(project, action, arguments.into()).map(|(_, artifacts)| artifacts)
}

fn execute_options(
    project: &Project,
    action: CargoAction,
    options: BuildOptions,
) -> Result<(ExecutionPlan, cargo_prototyper::ArtifactSet), cargo_prototyper::Error> {
    let plan = ExecutionPlan::resolve(project, action, options)?;
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

fn default_payload_options(firmware: &BuildOptions) -> BuildOptions {
    BuildOptions {
        role: ImageRole::Test,
        target: firmware.target.clone(),
        release: firmware.release,
        test_link_address: firmware.test_link_address.clone(),
        ..BuildOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_run_uses_the_matching_s_mode_test_payload() {
        let firmware = BuildOptions {
            target: Some("riscv32imac-unknown-none-elf".into()),
            release: false,
            ..BuildOptions::default()
        };
        let payload = default_payload_options(&firmware);
        assert_eq!(payload.role, ImageRole::Test);
        assert_eq!(payload.target, firmware.target);
        assert!(!payload.release);
        assert!(payload.payload.is_none());
    }
}
