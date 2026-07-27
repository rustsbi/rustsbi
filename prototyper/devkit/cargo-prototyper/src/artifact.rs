use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::{CargoAction, Error, ExecutionPlan, ImageRole, LinkInputContents};

/// Paths produced from one resolved image plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactSet {
    pub elf: PathBuf,
    pub binary: PathBuf,
    pub named_elf: Option<PathBuf>,
    pub named_binary: PathBuf,
    pub fit: Option<PathBuf>,
}

impl ArtifactSet {
    pub fn from_plan(plan: &ExecutionPlan) -> Self {
        let elf = plan.image.target_dir.join(plan.image.binary);
        let binary = plan
            .image
            .target_dir
            .join(format!("{}.bin", plan.image.binary));
        let named_elf = plan.image.mode_suffix.map(|suffix| {
            plan.image
                .target_dir
                .join(format!("{}-{suffix}.elf", plan.image.binary))
        });
        let named_binary = plan.image.mode_suffix.map_or_else(
            || binary.clone(),
            |suffix| {
                plan.image
                    .target_dir
                    .join(format!("{}-{suffix}.bin", plan.image.binary))
            },
        );
        let fit = plan.image.pack.then(|| {
            plan.image
                .target_dir
                .join(format!("{}.itb", plan.image.binary))
        });
        Self {
            elf,
            binary,
            named_elf,
            named_binary,
            fit,
        }
    }
}

/// Executes Cargo and, for build plans, constructs the final image artifacts.
pub fn execute(plan: &ExecutionPlan) -> Result<ArtifactSet, Error> {
    prepare_link_inputs(plan)?;
    let mut cargo = Command::new(&plan.program);
    cargo
        .current_dir(&plan.project_root)
        .args(&plan.arguments)
        .env_remove("RUSTSBI_MTEST_LIST")
        .env_remove("RUSTSBI_MTEST_FILTER")
        .envs(&plan.environment);
    run_checked(&mut cargo, &plan.program)?;

    let artifacts = ArtifactSet::from_plan(plan);
    if plan.action != CargoAction::Build {
        return Ok(artifacts);
    }
    fs::create_dir_all(&plan.image.target_dir)
        .map_err(|error| Error::io("create target directory", error))?;
    let architecture = if plan.image.target.contains("riscv32") {
        "riscv32"
    } else {
        "riscv64"
    };
    let mut objcopy = Command::new("rust-objcopy");
    objcopy.args([
        "-O",
        "binary",
        &format!("--binary-architecture={architecture}"),
        &artifacts.elf.to_string_lossy(),
        &artifacts.binary.to_string_lossy(),
    ]);
    run_checked(&mut objcopy, "rust-objcopy")?;

    if let Some(named_elf) = &artifacts.named_elf {
        fs::copy(&artifacts.elf, named_elf).map_err(|error| Error::io("copy named ELF", error))?;
    }
    if artifacts.named_binary != artifacts.binary {
        fs::copy(&artifacts.binary, &artifacts.named_binary)
            .map_err(|error| Error::io("copy named binary", error))?;
    }
    if plan.image.pack {
        pack_fit(plan, &artifacts)?;
    }
    Ok(artifacts)
}

fn prepare_link_inputs(plan: &ExecutionPlan) -> Result<(), Error> {
    let format = if plan.image.target.contains("riscv32") {
        "elf32-littleriscv"
    } else {
        "elf64-littleriscv"
    };
    for input in &plan.image.link_inputs {
        let parent = input
            .object
            .parent()
            .expect("generated object paths always have a parent");
        fs::create_dir_all(parent)
            .map_err(|error| Error::io("create link-input directory", error))?;
        let source = match &input.contents {
            LinkInputContents::File(path) => path.clone(),
            LinkInputContents::Bytes(bytes) => {
                let path = input.object.with_extension("bin");
                fs::write(&path, bytes)
                    .map_err(|error| Error::io("write normalized firmware contract", error))?;
                path
            }
        };
        let mut objcopy = Command::new("rust-objcopy");
        objcopy.args([
            "-I",
            "binary",
            "-O",
            format,
            "--rename-section",
            &format!(".data={},alloc,load,readonly,data,contents", input.section),
            &source.to_string_lossy(),
            &input.object.to_string_lossy(),
        ]);
        run_checked(&mut objcopy, "rust-objcopy")?;
    }
    Ok(())
}

fn pack_fit(plan: &ExecutionPlan, artifacts: &ArtifactSet) -> Result<(), Error> {
    let relative = match plan.image.role {
        ImageRole::Test => "prototyper/test-kernel/scripts/rustsbi-test-kernel.its",
        ImageRole::Bench => "prototyper/bench-kernel/scripts/rustsbi-bench-kernel.its",
        ImageRole::Firmware | ImageRole::Mtest => unreachable!("plan rejected this packaging role"),
    };
    let source = plan.project_root.join(relative);
    if !source.is_file() {
        return Err(Error::MissingInput(source));
    }
    let firmware = plan.image.target_dir.join("rustsbi-firmware.bin");
    if !firmware.is_file() {
        return Err(Error::MissingInput(firmware));
    }
    let its = plan
        .image
        .target_dir
        .join(format!("{}.its", plan.image.binary));
    fs::copy(&source, &its).map_err(|error| Error::io("copy FIT description", error))?;
    let fit = artifacts
        .fit
        .as_ref()
        .expect("pack plans always define a FIT output");
    let mut mkimage = Command::new("mkimage");
    mkimage.current_dir(&plan.image.target_dir).args([
        "-f",
        &its.file_name()
            .expect("generated ITS has a file name")
            .to_string_lossy(),
        &fit.file_name()
            .expect("generated FIT has a file name")
            .to_string_lossy(),
    ]);
    let result = run_checked(&mut mkimage, "mkimage");
    let cleanup = fs::remove_file(&its).map_err(|error| Error::io("remove temporary ITS", error));
    result.and(cleanup)
}

fn run_checked(command: &mut Command, program: &str) -> Result<(), Error> {
    let status = command
        .status()
        .map_err(|error| Error::io("start process", error))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::ProcessFailed {
            program: program.into(),
            code: status.code(),
        })
    }
}
