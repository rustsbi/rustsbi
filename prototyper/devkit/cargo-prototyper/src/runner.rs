use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{ArtifactSet, Error, ImagePlan};

/// QEMU machine model selected without exposing a shell command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QemuMachine {
    Virt,
}

impl QemuMachine {
    const fn argument(self) -> &'static str {
        match self {
            Self::Virt => "virt",
        }
    }
}

/// How the guest serial port is connected to the supervised process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerialMode {
    Captured,
}

/// Checked QEMU defaults that are independent of Cargo package layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QemuConfig {
    pub machine: QemuMachine,
    pub memory_mib: u32,
    pub harts: u16,
    pub serial: SerialMode,
    /// Additional argv entries are passed literally, never through a shell.
    pub extra_arguments: Vec<String>,
}

impl Default for QemuConfig {
    fn default() -> Self {
        Self {
            machine: QemuMachine::Virt,
            memory_mib: 128,
            harts: 1,
            serial: SerialMode::Captured,
            extra_arguments: Vec::new(),
        }
    }
}

/// A complete process launch request independent of Cargo and manifest logic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchPlan {
    pub program: String,
    pub arguments: Vec<String>,
    pub working_directory: PathBuf,
    pub timeout: Duration,
}

impl LaunchPlan {
    /// Creates the initial QEMU `virt` firmware launch contract.
    pub fn qemu_virt(
        workspace: PathBuf,
        image: &ImagePlan,
        artifacts: &ArtifactSet,
        config: &QemuConfig,
        timeout: Duration,
    ) -> Result<Self, Error> {
        if !matches!(
            image.role,
            crate::ImageRole::Firmware | crate::ImageRole::Mtest
        ) {
            return Err(Error::UnsupportedLaunchRole(image.role.name()));
        }
        if config.memory_mib == 0 {
            return Err(Error::InvalidRunnerConfiguration(
                "memory must be greater than zero",
            ));
        }
        if config.harts == 0 {
            return Err(Error::InvalidRunnerConfiguration(
                "hart count must be greater than zero",
            ));
        }
        let program = if image.target.contains("riscv32") {
            "qemu-system-riscv32"
        } else {
            "qemu-system-riscv64"
        };
        let firmware = artifacts.named_elf.as_ref().unwrap_or(&artifacts.elf);
        let mut arguments = vec![
            "-machine".into(),
            config.machine.argument().into(),
            "-m".into(),
            format!("{}M", config.memory_mib),
            "-smp".into(),
            config.harts.to_string(),
        ];
        match config.serial {
            SerialMode::Captured => arguments.extend([
                "-display".into(),
                "none".into(),
                "-serial".into(),
                "stdio".into(),
            ]),
        }
        arguments.extend(["-bios".into(), firmware.display().to_string()]);
        arguments.extend(config.extra_arguments.iter().cloned());
        Ok(Self {
            program: program.into(),
            arguments,
            working_directory: workspace,
            timeout,
        })
    }
}

/// Classified process completion with complete captured serial streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Exited {
        code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    TimedOut {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
}

impl Outcome {
    pub fn success(&self) -> bool {
        matches!(self, Self::Exited { code: Some(0), .. })
    }

    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::Exited {
                code: Some(code), ..
            } => *code,
            Self::Exited { code: None, .. } => 1,
            Self::TimedOut { .. } => 124,
        }
    }
}

/// Runs a resolved launch plan without inspecting build metadata.
pub fn run(plan: &LaunchPlan) -> Result<Outcome, Error> {
    let mut child = Command::new(&plan.program)
        .current_dir(&plan.working_directory)
        .args(&plan.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| Error::io("start runner", error))?;
    let stdout = child.stdout.take().expect("piped stdout is available");
    let stderr = child.stderr.take().expect("piped stderr is available");
    let stdout_reader = thread::spawn(move || read_all(stdout));
    let stderr_reader = thread::spawn(move || read_all(stderr));
    let deadline = Instant::now() + plan.timeout;

    let completion = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| Error::io("poll runner", error))?
        {
            break Ok(status.code());
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .map_err(|error| Error::io("terminate timed-out runner", error))?;
            child
                .wait()
                .map_err(|error| Error::io("reap timed-out runner", error))?;
            break Err(());
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout_reader
        .join()
        .expect("runner stdout reader must not panic")?;
    let stderr = stderr_reader
        .join()
        .expect("runner stderr reader must not panic")?;
    Ok(match completion {
        Ok(code) => Outcome::Exited {
            code,
            stdout,
            stderr,
        },
        Err(()) => Outcome::TimedOut { stdout, stderr },
    })
}

fn read_all(mut stream: impl Read) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| Error::io("capture runner output", error))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildOptions, FirmwareConfig, FirmwareType, ImageRole, NextStageSource, Project};

    #[test]
    fn qemu_arguments_follow_the_resolved_architecture_and_named_binary() {
        let project = Project::discover(env!("CARGO_MANIFEST_DIR")).unwrap();
        let artifacts = ArtifactSet {
            elf: "raw".into(),
            binary: "raw.bin".into(),
            named_elf: None,
            named_binary: "firmware.bin".into(),
            fit: None,
        };
        let image = crate::ImagePlan::resolve(
            &project,
            BuildOptions {
                role: ImageRole::Firmware,
                target: Some("riscv32imac-unknown-none-elf".into()),
                firmware: Some(FirmwareConfig {
                    source: "<test>".into(),
                    platform: "qemu-virt".into(),
                    firmware_type: FirmwareType::Dynamic,
                    device_tree: None,
                    next_stage: NextStageSource::Dynamic,
                }),
                ..BuildOptions::default()
            },
        )
        .unwrap();
        let plan = LaunchPlan::qemu_virt(
            "workspace".into(),
            &image,
            &artifacts,
            &QemuConfig::default(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(plan.program, "qemu-system-riscv32");
        assert_eq!(
            plan.arguments,
            [
                "-machine", "virt", "-m", "128M", "-smp", "1", "-display", "none", "-serial",
                "stdio", "-bios", "raw"
            ]
        );

        let stage = crate::ImagePlan::resolve(
            &project,
            BuildOptions {
                role: ImageRole::Test,
                ..BuildOptions::default()
            },
        )
        .unwrap();
        assert!(matches!(
            LaunchPlan::qemu_virt(
                "workspace".into(),
                &stage,
                &artifacts,
                &QemuConfig::default(),
                Duration::from_secs(2)
            ),
            Err(Error::UnsupportedLaunchRole("test"))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn classifies_exit_and_timeout_with_captured_output() {
        let exited = run(&LaunchPlan {
            program: "sh".into(),
            arguments: vec!["-c".into(), "printf ready".into()],
            working_directory: ".".into(),
            timeout: Duration::from_secs(1),
        })
        .unwrap();
        assert_eq!(
            exited,
            Outcome::Exited {
                code: Some(0),
                stdout: b"ready".to_vec(),
                stderr: Vec::new(),
            }
        );

        let timed_out = run(&LaunchPlan {
            program: "sh".into(),
            arguments: vec!["-c".into(), "printf waiting; sleep 1".into()],
            working_directory: ".".into(),
            timeout: Duration::from_millis(20),
        })
        .unwrap();
        assert!(matches!(timed_out, Outcome::TimedOut { .. }));
        assert_eq!(timed_out.exit_code(), 124);
    }
}
