// SPDX-License-Identifier: MulanPSL-2.0

use crate::{Options, Plan, Result, Step, read_toml};
use std::{collections::BTreeSet, path::Path, process::Command};

const LIBRARY: &str = ".github/workflows/Library.yml";
const PROTOTYPER: &str = ".github/workflows/prototyper.yml";
const RV64: &str = "riscv64imac-unknown-none-elf";
const RV32: &str = "riscv32imac-unknown-none-elf";
const FIRMWARE: &str = "riscv64gc-unknown-none-elf";

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

struct Builder {
    steps: Vec<Step>,
    tools: BTreeSet<String>,
    components: BTreeSet<String>,
    targets: BTreeSet<String>,
}

impl Builder {
    fn add(&mut self, id: &str, argv: Vec<String>, source: &str, artifacts: &[String], boot: bool) {
        if self.steps.iter().any(|step| step.id == id) {
            return;
        }
        self.steps.push(Step {
            id: id.into(),
            argv,
            source: source.into(),
            artifacts: artifacts.iter().map(Into::into).collect(),
            verifies_boot: boot,
        });
    }

    fn cargo(&mut self, id: &str, args: &[&str], source: &str) {
        self.add(
            id,
            std::iter::once("cargo")
                .chain(args.iter().copied())
                .map(Into::into)
                .collect(),
            source,
            &[],
            false,
        );
    }

    fn fmt(&mut self) {
        self.components.insert("rustfmt".into());
        self.cargo("fmt", &["fmt", "--all", "--", "--check"], LIBRARY);
    }

    fn firmware_tools(&mut self) {
        self.tools.insert("rust-objcopy".into());
        self.components
            .extend(["rust-src".into(), "llvm-tools-preview".into()]);
        self.targets.insert(FIRMWARE.into());
    }

    fn firmware(&mut self, mode: &str) {
        self.firmware_tools();
        self.add(
            &format!("firmware-{mode}"),
            vec![
                "cargo".into(),
                "prototyper".into(),
                "build".into(),
                mode.into(),
            ],
            PROTOTYPER,
            &[
                format!("target/{FIRMWARE}/release/rustsbi-prototyper-{mode}.elf"),
                format!("target/{FIRMWARE}/release/rustsbi-prototyper-{mode}.bin"),
            ],
            false,
        );
    }

    fn kernel(&mut self, kernel: &str, pack: bool, boot: bool) {
        self.firmware_tools();
        self.targets.insert(RV64.into());
        let mut argv = vec!["cargo".into(), "prototyper".into(), kernel.into()];
        let suffix = if pack {
            "pack"
        } else if boot {
            "boot"
        } else {
            "build"
        };
        if pack {
            argv.push("--pack".into());
        }
        if !boot {
            argv.push("--no-run".into());
        }
        let mut artifacts = vec![
            format!("target/{RV64}/release/rustsbi-{kernel}-kernel.bin"),
            format!("target/{FIRMWARE}/release/rustsbi-prototyper-payload-{kernel}.elf"),
        ];
        if pack {
            artifacts.push(format!("target/{RV64}/release/rustsbi-{kernel}-kernel.itb"));
        }
        self.add(
            &format!("{kernel}-{suffix}"),
            argv,
            PROTOTYPER,
            &artifacts,
            boot,
        );
    }

    fn library(&mut self) {
        self.fmt();
        for target in [RV64, RV32] {
            self.targets.insert(target.into());
            for feature in ["", "machine", "forward", "machine,forward"] {
                let mut args = vec![
                    "build",
                    "--locked",
                    "-p",
                    "rustsbi",
                    "--target",
                    target,
                    "--verbose",
                ];
                if !feature.is_empty() {
                    args.extend(["--features", feature]);
                }
                let label = if feature.is_empty() {
                    "default".into()
                } else {
                    feature.replace(',', "-")
                };
                self.cargo(&format!("rustsbi-{target}-{label}"), &args, LIBRARY);
            }
            self.cargo(
                &format!("sbi-rt-{target}"),
                &[
                    "build",
                    "--locked",
                    "-p",
                    "sbi-rt",
                    "--target",
                    target,
                    "--verbose",
                ],
                LIBRARY,
            );
            for feature in ["", "log"] {
                let mut args = vec![
                    "build",
                    "--locked",
                    "-p",
                    "sbi-testing",
                    "--target",
                    target,
                    "--verbose",
                ];
                if !feature.is_empty() {
                    args.extend(["--features", feature]);
                }
                let label = if feature.is_empty() {
                    "default"
                } else {
                    feature
                };
                self.cargo(&format!("sbi-testing-{target}-{label}"), &args, LIBRARY);
            }
        }
        self.cargo(
            "test-rustsbi",
            &["test", "--locked", "-p", "rustsbi", "--verbose"],
            LIBRARY,
        );
        self.cargo(
            "test-rustsbi-machine",
            &[
                "test",
                "--locked",
                "-p",
                "rustsbi",
                "--features",
                "machine",
                "--verbose",
            ],
            LIBRARY,
        );
        for package in ["riscv-aia", "sbi-spec", "rpmi", "penglai", "sbi-rt"] {
            self.cargo(
                &format!("test-{package}"),
                &["test", "--locked", "-p", package, "--verbose"],
                LIBRARY,
            );
        }
    }

    fn prototyper(&mut self) {
        self.fmt();
        self.firmware_tools();
        self.cargo(
            "test-xtask",
            &["test", "--locked", "-p", "xtask"],
            PROTOTYPER,
        );
        self.firmware("dynamic");
        self.cargo(
            "test-prototyper",
            &[
                "test",
                "--locked",
                "-p",
                "rustsbi-prototyper",
                "-p",
                "rustsbi-runtime-macros",
            ],
            PROTOTYPER,
        );
        self.cargo(
            "test-runtime",
            &[
                "test",
                "--locked",
                "-p",
                "rustsbi-firmware-runtime",
                "--lib",
            ],
            PROTOTYPER,
        );
        self.components.insert("clippy".into());
        self.targets.insert(RV64.into());
        for (package, target) in [
            ("rustsbi-prototyper", FIRMWARE),
            ("rustsbi-test-kernel", RV64),
            ("rustsbi-bench-kernel", RV64),
        ] {
            self.cargo(
                &format!("clippy-{package}"),
                &["clippy", "--locked", "-p", package, "--target", target],
                PROTOTYPER,
            );
        }
    }

    fn qemu(&mut self, output: &Path) -> Result<()> {
        self.tools.extend([
            "qemu-system-riscv64".into(),
            "bash".into(),
            "timeout".into(),
            "grep".into(),
            "sed".into(),
            "awk".into(),
            "seq".into(),
        ]);
        self.firmware("dynamic");
        self.firmware("jump");
        for kernel in ["test", "bench"] {
            self.kernel(kernel, false, false);
        }
        for kernel in ["test", "bench"] {
            self.kernel(kernel, false, true);
            for mode in ["dynamic", "jump"] {
                self.add(
                    &format!("{mode}-{kernel}-boot"),
                    vec![
                        "bash".into(),
                        ".github/scripts/prototyper-qemu-boot.sh".into(),
                        mode.into(),
                        kernel.into(),
                        output
                            .join("qemu-logs")
                            .to_str()
                            .ok_or("non-UTF-8 output path")?
                            .into(),
                    ],
                    PROTOTYPER,
                    &[],
                    true,
                );
            }
        }
        Ok(())
    }

    fn pack(&mut self) {
        self.tools.extend(["mkimage".into(), "dtc".into()]);
        for kernel in ["test", "bench"] {
            self.kernel(kernel, true, false);
        }
    }
}

pub(super) fn resolve(options: Options) -> Result<Plan> {
    let manifest = read_toml(&options.repo.join("rust-toolchain.toml"))?;
    let toolchain = manifest
        .get("toolchain")
        .and_then(|value| value.get("channel"))
        .and_then(toml::Value::as_str)
        .ok_or("rust-toolchain.toml has no toolchain.channel")?
        .to_string();
    let mut builder = Builder {
        steps: vec![],
        tools: ["cargo", "rustc", "rustup", "cc"]
            .into_iter()
            .map(Into::into)
            .collect(),
        components: BTreeSet::new(),
        targets: BTreeSet::new(),
    };
    match options.suite.as_str() {
        "build" => {
            builder.cargo(
                "build-host",
                &["build", "--locked"],
                "Cargo.toml#workspace.default-members",
            );
            builder.firmware("dynamic");
        }
        "library" => builder.library(),
        "prototyper" => builder.prototyper(),
        "qemu" => builder.qemu(&options.output)?,
        "pack" => builder.pack(),
        "all" => {
            builder.library();
            builder.prototyper();
            builder.qemu(&options.output)?;
            builder.pack();
        }
        _ => unreachable!(),
    }
    for step in &builder.steps {
        if step.argv.first().is_some_and(|s| s == "bash")
            && !options.repo.join(&step.argv[1]).is_file()
        {
            return Err(format!("missing repository script: {}", step.argv[1]).into());
        }
    }
    let head = git(&options.repo, &["rev-parse", "HEAD"]);
    let branch = git(&options.repo, &["symbolic-ref", "--short", "HEAD"]);
    let dirty = git(&options.repo, &["status", "--porcelain"]).is_none_or(|s| !s.is_empty());
    Ok(Plan {
        schema_version: 1,
        mode: options.mode,
        suite: options.suite,
        target_dir: options.repo.join("target"),
        repo: options.repo,
        output: options.output,
        toolchain,
        head,
        branch,
        dirty,
        qemu_bin: options.qemu_bin,
        required_tools: builder.tools.into_iter().collect(),
        required_components: builder.components.into_iter().collect(),
        required_targets: builder.targets.into_iter().collect(),
        steps: builder.steps,
    })
}
