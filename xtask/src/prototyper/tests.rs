use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::Parser;
use tempfile::TempDir;

use super::{
    BuildArgs, BuildMode, BuildPaths, FirmwareLayout, PrototyperCommand, Target,
    build::remove_stale_payload_artifacts,
    generate_build_inputs,
    kernels::{self, Kernel, KernelArgs, ResolvedRun, forbidden_patterns},
    qemu::{Attempt, NextStep, next_step, verify_output},
    render_linker_script, resolve_in,
    scheme::{Action, Scheme},
};
use crate::utils::cargo_target_dir_in;

const VALID_CONFIG_TOML: &str = "link_start_address = 0x80000000\n\
                                  heap_size = 0x15000\n\
                                  payload_address = 0x80200000\n\
                                  jump_address = 0x80200000\n";
const LINKER_TEMPLATE: &str = r#". = @LINK_START_ADDRESS@;
.bss : {
    sbi_boot_stack_start = .;
    . += @STACK_SIZE_PER_HART@;
    sbi_boot_stack_end = .;
    sbi_heap_start = .;
    . += @HEAP_SIZE@;
    sbi_heap_end = .;
}
.payload @PAYLOAD_ADDRESS@ : ALIGN(0x1000) { *(.payload) }
"#;

#[derive(Parser)]
struct TestCli {
    #[command(subcommand)]
    command: PrototyperCommand,
}

fn parse(args: &[&str]) -> std::result::Result<PrototyperCommand, clap::Error> {
    TestCli::try_parse_from(args).map(|cli| cli.command)
}

fn parse_build(args: &[&str]) -> std::result::Result<BuildArgs, clap::Error> {
    match parse(args)? {
        PrototyperCommand::Build(args) => Ok(args),
        _ => panic!("expected `build` subcommand"),
    }
}

fn base_build_args() -> BuildArgs {
    BuildArgs {
        mode: None,
        features: Vec::new(),
        no_default_features: false,
        fdt: None,
        debug: false,
        config_file: None,
        target: None,
    }
}

#[test]
fn cli_parses_commands_and_build_arguments() {
    assert!(parse(&["prototyper"]).is_err());

    let args = parse_build(&[
        "prototyper",
        "build",
        "--features",
        "hypervisor",
        "--no-default-features",
        "--fdt",
        "board.dtb",
        "--debug",
        "--config-file",
        "custom.toml",
        "--target",
        "riscv64gc-unknown-none-elf",
        "jump",
    ])
    .unwrap();
    assert_eq!(args.mode, Some(BuildMode::Jump));
    assert_eq!(args.features, ["hypervisor"]);
    assert!(args.no_default_features);
    assert!(
        !parse_build(&["prototyper", "build"])
            .unwrap()
            .no_default_features
    );
    assert_eq!(args.fdt, Some(PathBuf::from("board.dtb")));
    assert!(args.debug);
    assert_eq!(args.config_file, Some(PathBuf::from("custom.toml")));
    assert_eq!(args.target.as_deref(), Some("riscv64gc-unknown-none-elf"));
    assert_eq!(parse_build(&["prototyper", "build"]).unwrap().mode, None);
    assert_eq!(
        parse_build(&["prototyper", "build", "dynamic"])
            .unwrap()
            .mode,
        Some(BuildMode::Dynamic)
    );
    assert!(matches!(
        parse_build(&["prototyper", "build", "payload", "kernel.bin"])
            .unwrap()
            .mode,
        Some(BuildMode::Payload { .. })
    ));

    match parse(&["prototyper", "test", "--pack"]).unwrap() {
        PrototyperCommand::Test(args) => assert!(args.pack),
        _ => panic!("expected `test` subcommand"),
    }
    match parse(&["prototyper", "bench"]).unwrap() {
        PrototyperCommand::Bench(args) => assert!(!args.pack),
        _ => panic!("expected `bench` subcommand"),
    }
}

#[test]
fn cli_kernel_options_flow_and_resolve_via_scheme() {
    // Explicit flags flow through verbatim as Some(_).
    let args = match parse(&[
        "prototyper",
        "test",
        "--no-run",
        "--smp",
        "2",
        "--timeout",
        "30",
        "--retries",
        "1",
    ])
    .unwrap()
    {
        PrototyperCommand::Test(args) => args,
        _ => panic!("expected `test` subcommand"),
    };
    assert!(args.no_run);
    assert_eq!(args.smp, Some(2));
    assert_eq!(args.timeout, Some(30));
    assert_eq!(args.retries, Some(1));

    // Absent flags are None; resolution happens against the Scheme.
    let scheme = Scheme::default();
    let args = match parse(&["prototyper", "bench"]).unwrap() {
        PrototyperCommand::Bench(args) => args,
        _ => panic!("expected `bench` subcommand"),
    };
    assert_eq!(args.smp, None);
    let resolved_smp = args.smp.unwrap_or(scheme.action(Action::Bench).smp);
    assert_eq!(resolved_smp, 4);
    assert!(!args.no_run);
}

#[test]
fn qemu_output_verification_checks_expected_and_forbidden_patterns() {
    let expected = vec![
        "Hello RustSBI!".to_string(),
        "Platform HART Count           : 1".to_string(),
        "Sbi `Base` test pass".to_string(),
    ];
    let forbidden = vec!["panicked".to_string(), "FAILED".to_string()];
    let passing = "RustSBI version\n\
                   Hello RustSBI!\n\
                   Platform HART Count           : 1\n\
                   Sbi `Base` test pass\n";
    assert!(verify_output(passing, &expected, &forbidden).is_ok());

    let missing = verify_output("Hello RustSBI!", &expected, &forbidden).unwrap_err();
    assert!(format!("{missing:#}").contains("Platform HART Count"));

    let failure = verify_output(
        "Hello RustSBI!\nPlatform HART Count           : 1\nSbi `Base` test pass\npanicked at 'oops'",
        &expected,
        &forbidden,
    )
    .unwrap_err();
    assert!(format!("{failure:#}").contains("panic"));

    assert!(verify_output("", &expected, &forbidden).is_err());
}

#[test]
fn scheme_defaults_drive_kernel_runs() {
    let scheme = Scheme::default();
    // Defaults come from `scheme.rs`, independently of explicit CLI flags.
    assert_eq!(scheme.action(Action::Test).smp, 1);
    assert_eq!(scheme.action(Action::Bench).smp, 4);
    assert_eq!(scheme.action(Action::Bench).timeout_secs, 90);
    assert_eq!(scheme.qemu.machine, "virt");
    assert_eq!(scheme.qemu.memory_mb, 256);
}

#[test]
fn qemu_retries_only_after_timeout() {
    let clean = |success: bool| Attempt::Exited {
        success,
        output: String::new(),
    };
    let timed_out = || Attempt::TimedOut {
        output: String::new(),
    };

    // A clean exit with verified output passes on the first attempt.
    assert_eq!(next_step(&clean(true), Some(true), true), NextStep::Pass);
    assert_eq!(next_step(&clean(true), Some(true), false), NextStep::Pass);

    // A verification failure fails immediately, even with attempts left.
    assert_eq!(
        next_step(&clean(true), Some(false), true),
        NextStep::VerificationFailed
    );

    // A non-zero exit fails immediately, even with attempts left.
    assert_eq!(next_step(&clean(false), None, true), NextStep::NonZeroExit);

    // A timeout retries only while attempts remain.
    assert_eq!(next_step(&timed_out(), None, true), NextStep::Retry);
    assert_eq!(next_step(&timed_out(), None, false), NextStep::TimedOut);
}

#[test]
fn console_pattern_files_drive_qemu_verification() {
    // xtask and `.github/scripts/prototyper-qemu-boot.sh` share these pattern
    // files. They must parse, substitute `{smp}`, and retain the required output
    // and failure patterns.
    let test_patterns = Kernel::Test.expected_patterns(4).unwrap();
    assert!(test_patterns.contains(&"Hello RustSBI!".to_string()));
    assert!(test_patterns.contains(&"Platform HART Count           : 4".to_string()));
    assert!(test_patterns.contains(&"Sbi `TIME` test pass".to_string()));
    assert!(test_patterns.contains(&"[pmu] counters number:".to_string()));

    let bench_patterns = Kernel::Bench.expected_patterns(1).unwrap();
    assert!(bench_patterns.contains(&"Platform HART Count           : 1".to_string()));
    assert!(bench_patterns.contains(&"Test #3:".to_string()));

    let forbidden = forbidden_patterns().unwrap();
    assert!(forbidden.contains(&"panicked".to_string()));
    assert!(forbidden.contains(&"FAILED".to_string()));
    assert!(forbidden.contains(&"SystemFailure".to_string()));
}

#[test]
fn resolve_normalizes_files_and_derives_features() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let config_dir = root.join("firmware/prototyper/config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("default.toml"), VALID_CONFIG_TOML).unwrap();
    fs::write(root.join("kernel.bin"), b"kernel").unwrap();
    fs::write(root.join("board.dtb"), b"dtb").unwrap();
    let args = BuildArgs {
        mode: Some(BuildMode::Payload {
            path: PathBuf::from("kernel.bin"),
        }),
        fdt: Some(PathBuf::from("board.dtb")),
        features: vec![" hypervisor, serde ".to_string()],
        ..base_build_args()
    };

    let spec = resolve_in(&args, root, root).unwrap();
    assert_eq!(spec.firmware_config.layout.hart_capacity, 8);
    assert_eq!(spec.firmware_config.layout.stack_size_per_hart, 16384);
    assert_eq!(
        spec.mode,
        BuildMode::Payload {
            path: root.join("kernel.bin")
        }
    );
    assert_eq!(spec.fdt, Some(root.join("board.dtb")));
    assert_eq!(
        spec.cargo_features(),
        ["hypervisor", "serde", "fdt", "payload"]
    );

    fs::write(
        config_dir.join("default.toml"),
        format!("{VALID_CONFIG_TOML}num_hart_max = 2\nstack_size_per_hart = 8192\n"),
    )
    .unwrap();
    let spec = resolve_in(&args, root, root).unwrap();
    assert_eq!(spec.firmware_config.layout.hart_capacity, 2);
    assert_eq!(spec.firmware_config.layout.stack_size_per_hart, 8192);
    temp.close().unwrap();
}

#[test]
fn resolve_rejects_mode_features_and_invalid_config() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let config_dir = root.join("firmware/prototyper/config");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("default.toml");
    fs::write(&config_path, VALID_CONFIG_TOML).unwrap();
    for feature in ["payload", "jump", "jump ", "fdt"] {
        let args = BuildArgs {
            features: vec![feature.to_string()],
            ..base_build_args()
        };
        assert!(
            resolve_in(&args, root, root).is_err(),
            "accepted feature {feature}"
        );
    }

    fs::write(
        &config_path,
        "link_start_address = 0x80000000\njump_address = 0x80200000\n",
    )
    .unwrap();
    let error = resolve_in(&base_build_args(), root, root).unwrap_err();
    assert!(format!("{error:#}").contains("`payload_address`"));

    for heap_size in ["", "heap_size = 31"] {
        let config = VALID_CONFIG_TOML.replace("heap_size = 0x15000", heap_size);
        fs::write(&config_path, config).unwrap();
        let error = resolve_in(&base_build_args(), root, root).unwrap_err();
        assert!(format!("{error:#}").contains("`heap_size`"));
    }

    for (key, value) in [("num_hart_max", "0"), ("stack_size_per_hart", "-128")] {
        fs::write(
            &config_path,
            format!("{VALID_CONFIG_TOML}{key} = {value}\n"),
        )
        .unwrap();
        let error = resolve_in(&base_build_args(), root, root).unwrap_err();
        assert!(format!("{error:#}").contains(key));
    }

    fs::write(
        &config_path,
        "link_start_address = 0x80200000\n\
         heap_size = 0x15000\n\
         payload_address = 0x80000000\n\
         jump_address = 0x80200000\n",
    )
    .unwrap();
    let error = resolve_in(&base_build_args(), root, root).unwrap_err();
    assert!(format!("{error:#}").contains("must be less than"));
    temp.close().unwrap();
}

#[test]
fn resolve_derives_target_profile_and_rustflags() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let config_dir = root.join("firmware/prototyper/config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("default.toml"), VALID_CONFIG_TOML).unwrap();
    let target = root.join("custom-target.json");
    fs::write(&target, "{}").unwrap();
    let args = BuildArgs {
        target: Some(target.to_string_lossy().into_owned()),
        debug: true,
        features: vec!["hypervisor,serde".to_string()],
        ..base_build_args()
    };
    let spec = resolve_in(&args, root, root).unwrap();
    assert_eq!(
        spec.custom_target.as_deref(),
        Some(target.to_str().unwrap())
    );
    assert_eq!(
        spec.artifact_dir_in(&root.join("target")),
        root.join("target/custom-target/debug")
    );
    let encoded_rustflags = spec.encoded_rustflags(Path::new("linker path.ld"));
    assert!(encoded_rustflags.contains("+h"));
    assert!(
        encoded_rustflags
            .split('\u{1f}')
            .any(|flag| flag == "link-arg=-Tlinker path.ld")
    );
    temp.close().unwrap();
}

#[test]
fn generated_inputs_and_stamp_follow_build_mode() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let config_dir = root.join("firmware/prototyper/config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("default.toml"), VALID_CONFIG_TOML).unwrap();
    let linker_template = root.join("firmware/prototyper/rustsbi-prototyper.ld.in");
    fs::write(&linker_template, LINKER_TEMPLATE).unwrap();
    let paths = BuildPaths {
        target_dir: root.join("target"),
        artifact_dir: root
            .join("target")
            .join(Target::Firmware.triple())
            .join("release"),
        build_inputs_dir: root.join("target/prototyper"),
        linker_template,
    };
    assert_eq!(
        paths.linker_script(),
        root.join("target/prototyper/rustsbi-prototyper.ld")
    );
    let dynamic = resolve_in(&base_build_args(), root, root).unwrap();
    generate_build_inputs(&dynamic, &paths).unwrap();
    assert_eq!(
        fs::read_to_string(paths.build_inputs_dir.join("config.toml")).unwrap(),
        VALID_CONFIG_TOML
    );
    assert!(
        fs::read_to_string(paths.config_source())
            .unwrap()
            .contains(paths.build_inputs_dir.join("config.toml").to_str().unwrap())
    );
    assert!(
        fs::read_to_string(paths.linker_script())
            .unwrap()
            .contains("sbi_boot_stack_start = .;")
    );
    let generated_config = fs::read_to_string(paths.config_source()).unwrap();
    assert!(generated_config.contains("pub(crate) const HART_CAPACITY: usize = 8;"));
    assert!(generated_config.contains("pub(crate) const STACK_SIZE_PER_HART: usize = 16384;"));
    let dynamic_stamp = fs::read_to_string(paths.stamp()).unwrap();
    fs::write(
        config_dir.join("default.toml"),
        format!("{VALID_CONFIG_TOML}num_hart_max = 1\n"),
    )
    .unwrap();
    let custom = resolve_in(&base_build_args(), root, root).unwrap();
    generate_build_inputs(&custom, &paths).unwrap();
    assert!(
        fs::read_to_string(paths.config_source())
            .unwrap()
            .contains("pub(crate) const HART_CAPACITY: usize = 1;")
    );
    assert_ne!(dynamic_stamp, fs::read_to_string(paths.stamp()).unwrap());
    fs::write(config_dir.join("default.toml"), VALID_CONFIG_TOML).unwrap();
    let minimal = resolve_in(
        &BuildArgs {
            no_default_features: true,
            ..base_build_args()
        },
        root,
        root,
    )
    .unwrap();
    assert!(minimal.no_default_features);
    generate_build_inputs(&minimal, &paths).unwrap();
    assert_ne!(dynamic_stamp, fs::read_to_string(paths.stamp()).unwrap());
    assert!(!paths.payload_data().exists());
    assert!(!paths.fdt_data().exists());

    let payload = root.join("kernel.bin");
    let fdt = root.join("board.dtb");
    fs::write(&payload, b"kernel-bytes").unwrap();
    fs::write(&fdt, b"dtb").unwrap();
    let args = BuildArgs {
        mode: Some(BuildMode::Payload {
            path: payload.clone(),
        }),
        fdt: Some(fdt.clone()),
        ..base_build_args()
    };
    let payload_build = resolve_in(&args, root, root).unwrap();
    generate_build_inputs(&payload_build, &paths).unwrap();
    let payload_stamp = fs::read_to_string(paths.stamp()).unwrap();
    assert_ne!(dynamic_stamp, payload_stamp);
    assert_eq!(fs::read(paths.payload_data()).unwrap(), b"kernel-bytes");
    assert_eq!(fs::read(paths.fdt_data()).unwrap(), b"dtb");

    // Replacing an image at the same path must invalidate the Cargo stamp.
    fs::write(&payload, b"changed-bytes").unwrap();
    generate_build_inputs(&payload_build, &paths).unwrap();
    assert_ne!(payload_stamp, fs::read_to_string(paths.stamp()).unwrap());
    assert_eq!(fs::read(paths.payload_data()).unwrap(), b"changed-bytes");
    let changed_payload_stamp = fs::read_to_string(paths.stamp()).unwrap();
    fs::write(&fdt, b"new-dtb").unwrap();
    generate_build_inputs(&payload_build, &paths).unwrap();
    assert_ne!(
        changed_payload_stamp,
        fs::read_to_string(paths.stamp()).unwrap()
    );
    assert_eq!(fs::read(paths.fdt_data()).unwrap(), b"new-dtb");

    // Dynamic/jump builds must not reuse objects left by a payload/FDT build.
    fs::write(paths.payload_object(), b"previous-object").unwrap();
    fs::write(paths.fdt_object(), b"previous-object").unwrap();
    generate_build_inputs(&dynamic, &paths).unwrap();
    assert!(!paths.payload_data().exists());
    assert!(!paths.payload_object().exists());
    assert!(!paths.fdt_data().exists());
    assert!(!paths.fdt_object().exists());
    assert_eq!(dynamic_stamp, fs::read_to_string(paths.stamp()).unwrap());

    fs::write(&payload, b"").unwrap();
    assert!(
        generate_build_inputs(&payload_build, &paths)
            .unwrap_err()
            .to_string()
            .contains("is empty")
    );
    temp.close().unwrap();
}

#[test]
fn linker_template_renders_known_addresses_and_rejects_unknown_tokens() {
    let addresses = FirmwareLayout {
        link_start_address: 0x80000000,
        heap_size_bytes: 0x15000,
        payload_address: 0x80200000,
        hart_capacity: 2,
        stack_size_per_hart: 8192,
    };
    let rendered =
        render_linker_script(LINKER_TEMPLATE, &addresses, Some(addresses.payload_address)).unwrap();
    assert!(rendered.contains("0x80000000"));
    assert!(rendered.contains("0x80200000"));
    assert!(rendered.contains(". += 0x15000;"));
    assert!(rendered.contains(". += 0x2000;"));
    assert!(rendered.contains("sbi_boot_stack_end = .;"));
    assert!(rendered.contains(".payload 0x80200000 :"));

    let rendered_without_payload = render_linker_script(LINKER_TEMPLATE, &addresses, None).unwrap();
    assert!(!rendered_without_payload.contains("0x80200000"));
    assert!(rendered_without_payload.contains(".payload  :"));

    // Placeholder-shaped unknown tokens are rejected.
    let error = render_linker_script(". = @UNKNOWN@;", &addresses, None).unwrap_err();
    assert!(format!("{error:#}").contains("@UNKNOWN@"));

    // Literal `@` characters (e.g. in comments) are not placeholders.
    let rendered = render_linker_script("/* report bugs to dev@example.com */\n", &addresses, None)
        .expect("literal @ must not be rejected");
    assert!(rendered.contains("dev@example.com"));
    let rendered = render_linker_script("/* v2.0 @ 2026 */\n", &addresses, None)
        .expect("lowercase tokens must not be rejected");
    assert!(rendered.contains("@ 2026"));
    assert!(render_linker_script("@@", &addresses, None).is_ok());
}

#[test]
fn run_options_validation_rejects_zero_values_before_building() {
    use super::kernels::ResolvedRun;
    let valid = ResolvedRun {
        no_run: true,
        smp: 1,
        timeout_secs: 60,
        attempts: 1,
    };
    assert!(valid.validate().is_ok());

    let zero_retries = ResolvedRun {
        attempts: 0,
        ..valid
    };
    let error = zero_retries.validate().unwrap_err();
    assert!(format!("{error:#}").contains("--retries 0"));

    let zero_smp = ResolvedRun { smp: 0, ..valid };
    let error = zero_smp.validate().unwrap_err();
    assert!(format!("{error:#}").contains("--smp 0"));

    let zero_timeout = ResolvedRun {
        timeout_secs: 0,
        ..valid
    };
    let error = zero_timeout.validate().unwrap_err();
    assert!(format!("{error:#}").contains("--timeout 0"));
}

#[test]
fn cargo_target_dir_honors_env_override() {
    let cwd = Path::new("/workspace");
    assert_eq!(
        cargo_target_dir_in(Some("build-out".into()), cwd),
        PathBuf::from("/workspace/build-out")
    );
    assert_eq!(
        cargo_target_dir_in(Some("/abs/out".into()), cwd),
        PathBuf::from("/abs/out")
    );
    // Without the override, the default lives under the workspace root;
    // xtask is always built from this workspace, so the root exists.
    let default = cargo_target_dir_in(None, cwd);
    assert!(default.ends_with("target"));
    assert!(default.is_absolute());
}

#[test]
fn stale_generic_payload_artifacts_are_removed_for_suffixed_payload_builds() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::create_dir_all(root).unwrap();
    for extension in ["elf", "bin"] {
        fs::write(
            root.join(format!("rustsbi-prototyper-payload.{extension}")),
            b"stale",
        )
        .unwrap();
        fs::write(
            root.join(format!("rustsbi-prototyper-dynamic.{extension}")),
            b"keep",
        )
        .unwrap();
    }

    remove_stale_payload_artifacts(root, "payload-test").unwrap();
    assert!(!root.join("rustsbi-prototyper-payload.elf").exists());
    assert!(!root.join("rustsbi-prototyper-payload.bin").exists());
    // Dynamic artifacts are side-by-side outputs and must survive.
    assert!(root.join("rustsbi-prototyper-dynamic.elf").exists());

    // A plain payload build keeps its own artifacts; missing files are fine.
    for extension in ["elf", "bin"] {
        fs::write(
            root.join(format!("rustsbi-prototyper-payload.{extension}")),
            b"fresh",
        )
        .unwrap();
    }
    remove_stale_payload_artifacts(root, "payload").unwrap();
    assert!(root.join("rustsbi-prototyper-payload.elf").exists());
    remove_stale_payload_artifacts(root, "jump").unwrap();
    remove_stale_payload_artifacts(root, "payload-bench").unwrap();
    assert!(!root.join("rustsbi-prototyper-payload.elf").exists());
    temp.close().unwrap();
}

#[test]
fn resolved_run_prefers_cli_overrides_and_falls_back_to_scheme() {
    let scheme = Scheme::default();
    let args = KernelArgs {
        pack: false,
        no_run: true,
        smp: Some(8),
        timeout: Some(30),
        retries: Some(1),
        debug: false,
        config_file: None,
    };
    let run = ResolvedRun::resolve(&args, Kernel::Test, &scheme);
    assert_eq!((run.smp, run.timeout_secs, run.attempts), (8, 30, 1));

    // Absent flags fall back to the per-kernel scheme section.
    let args = KernelArgs {
        smp: None,
        timeout: None,
        retries: None,
        ..args
    };
    let test = ResolvedRun::resolve(&args, Kernel::Test, &scheme);
    assert_eq!((test.smp, test.timeout_secs, test.attempts), (1, 60, 2));
    let bench = ResolvedRun::resolve(&args, Kernel::Bench, &scheme);
    assert_eq!((bench.smp, bench.timeout_secs, bench.attempts), (4, 90, 4));
}

#[test]
fn pattern_files_must_yield_at_least_one_pattern() {
    // Fail closed: an emptied pattern file must not silently verify nothing
    // (mirrors the CI script's `test -s` guard).
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let dir = root.join("firmware/scripts");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("empty.txt"), "# only comments\n\n").unwrap();

    let err = kernels::read_console_patterns(&dir.join("empty.txt")).unwrap_err();
    assert!(format!("{err:#}").contains("no patterns"));

    fs::write(dir.join("ok.txt"), "  Hello  \n# note\n").unwrap();
    assert_eq!(
        kernels::read_console_patterns(&dir.join("ok.txt")).unwrap(),
        vec!["Hello".to_string()]
    );
    temp.close().unwrap();
}
