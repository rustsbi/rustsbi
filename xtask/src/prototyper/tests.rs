use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use clap::Parser;

use super::{
    ARCH, BuildArgs, BuildMode, BuildPaths, PlatformAddresses, PrototyperCommand,
    build::remove_stale_generic_payload_artifacts,
    generate_build_inputs,
    kernels::{Kernel, QemuOptions, forbidden_patterns},
    qemu::verify_output,
    render_linker_script, resolve_in,
};
use crate::utils::cargo_target_dir_in;

static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(0);

const VALID_CONFIG_TOML: &str = "link_start_address = 0x80000000\n\
                                  payload_address = 0x80200000\n\
                                  jump_address = 0x80200000\n";
const LINKER_TEMPLATE: &str =
    ". = @LINK_START_ADDRESS@;\n.text @PAYLOAD_ADDRESS@ : ALIGN(0x1000) { *(.payload) }\n";

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
fn cli_parses_kernel_qemu_arguments_and_defaults() {
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
    assert_eq!(args.smp, 2);
    assert_eq!(args.timeout, 30);
    assert_eq!(args.retries, 1);

    let args = match parse(&["prototyper", "test"]).unwrap() {
        PrototyperCommand::Test(args) => args,
        _ => panic!("expected `test` subcommand"),
    };
    assert!(!args.no_run);
    assert_eq!(args.smp, 1);
    assert_eq!(args.timeout, 60);
    assert_eq!(args.retries, 2);

    let args = match parse(&["prototyper", "bench"]).unwrap() {
        PrototyperCommand::Bench(args) => args,
        _ => panic!("expected `bench` subcommand"),
    };
    assert!(!args.no_run);
    assert_eq!(args.smp, 4);
    assert_eq!(args.timeout, 90);
    assert_eq!(args.retries, 4);
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
fn console_pattern_files_drive_qemu_verification() {
    // The shared pattern files under `prototyper/` are the single source for
    // xtask and `.github/scripts/prototyper-qemu-boot.sh`; they must parse,
    // substitute `{smp}`, and keep the load-bearing patterns.
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
    let root = env::temp_dir().join(format!(
        "xtask-prototyper-test-{}-{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    let config_dir = root.join("prototyper/prototyper/config");
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

    let spec = resolve_in(&args, &root, &root).unwrap();
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
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn resolve_rejects_mode_features_and_invalid_config() {
    let root = env::temp_dir().join(format!(
        "xtask-prototyper-test-{}-{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    let config_dir = root.join("prototyper/prototyper/config");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("default.toml");
    fs::write(&config_path, VALID_CONFIG_TOML).unwrap();
    for feature in ["payload", "jump", "jump ", "fdt"] {
        let args = BuildArgs {
            features: vec![feature.to_string()],
            ..base_build_args()
        };
        assert!(
            resolve_in(&args, &root, &root).is_err(),
            "accepted feature {feature}"
        );
    }

    fs::write(
        &config_path,
        "link_start_address = 0x80000000\njump_address = 0x80200000\n",
    )
    .unwrap();
    let error = resolve_in(&base_build_args(), &root, &root).unwrap_err();
    assert!(format!("{error:#}").contains("`payload_address`"));

    fs::write(
        &config_path,
        "link_start_address = 0x80200000\n\
         payload_address = 0x80000000\n\
         jump_address = 0x80200000\n",
    )
    .unwrap();
    let error = resolve_in(&base_build_args(), &root, &root).unwrap_err();
    assert!(format!("{error:#}").contains("must be less than"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn resolve_derives_target_profile_and_rustflags() {
    let root = env::temp_dir().join(format!(
        "xtask-prototyper-test-{}-{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    let config_dir = root.join("prototyper/prototyper/config");
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
    let spec = resolve_in(&args, &root, &root).unwrap();
    assert_eq!(spec.target_triple, "custom-target");
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
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn generated_inputs_and_stamp_follow_build_mode() {
    let root = env::temp_dir().join(format!(
        "xtask-prototyper-test-{}-{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    let config_dir = root.join("prototyper/prototyper/config");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("default.toml"), VALID_CONFIG_TOML).unwrap();
    let linker_template = root.join("prototyper/prototyper/rustsbi-prototyper.ld.in");
    fs::write(&linker_template, LINKER_TEMPLATE).unwrap();
    let paths = BuildPaths {
        artifact_dir: root.join("target").join(ARCH).join("release"),
        build_inputs_dir: root.join("target/prototyper"),
        linker_template,
    };
    assert_eq!(
        paths.linker_script_argument(),
        root.join("target/prototyper/rustsbi-prototyper.ld")
    );
    let dynamic = resolve_in(&base_build_args(), &root, &root).unwrap();
    generate_build_inputs(&dynamic, &paths).unwrap();
    let dynamic_stamp = fs::read_to_string(paths.stamp()).unwrap();
    assert!(
        fs::read_to_string(paths.alignment_source())
            .unwrap()
            .contains("Aligned16")
    );
    assert!(
        fs::read_to_string(paths.payload_source())
            .unwrap()
            .is_empty()
    );
    assert!(fs::read_to_string(paths.fdt_source()).unwrap().is_empty());

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
    let payload_build = resolve_in(&args, &root, &root).unwrap();
    generate_build_inputs(&payload_build, &paths).unwrap();
    let payload_stamp = fs::read_to_string(paths.stamp()).unwrap();
    let payload_source = fs::read_to_string(paths.payload_source()).unwrap();
    let fdt_source = fs::read_to_string(paths.fdt_source()).unwrap();
    assert_ne!(dynamic_stamp, payload_stamp);
    assert!(payload_source.contains("pub static payload_image"));
    assert!(payload_source.contains(&payload.display().to_string()));
    assert!(fdt_source.contains("pub static raw_fdt"));
    assert!(fdt_source.contains(&fdt.display().to_string()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn linker_template_renders_known_addresses_and_rejects_unknown_tokens() {
    let addresses = PlatformAddresses {
        link_start_address: 0x80000000,
        payload_address: 0x80200000,
    };
    let rendered = render_linker_script(
        ". = @LINK_START_ADDRESS@; .text @PAYLOAD_ADDRESS@ : { *(.payload) }",
        &addresses,
    )
    .unwrap();
    assert!(rendered.contains("0x80000000"));
    assert!(rendered.contains("0x80200000"));

    // Placeholder-shaped unknown tokens are rejected.
    let error = render_linker_script(". = @UNKNOWN@;", &addresses).unwrap_err();
    assert!(format!("{error:#}").contains("@UNKNOWN@"));

    // Literal `@` characters (e.g. in comments) are not placeholders.
    let rendered = render_linker_script("/* report bugs to dev@example.com */\n", &addresses)
        .expect("literal @ must not be rejected");
    assert!(rendered.contains("dev@example.com"));
    let rendered = render_linker_script("/* v2.0 @ 2026 */\n", &addresses)
        .expect("lowercase tokens must not be rejected");
    assert!(rendered.contains("@ 2026"));
    assert!(render_linker_script("@@", &addresses).is_ok());
}

#[test]
fn qemu_options_validation_rejects_zero_values_before_building() {
    let valid = QemuOptions {
        no_run: true,
        smp: 1,
        timeout_secs: 60,
        attempts: 1,
    };
    assert!(valid.validate().is_ok());

    let zero_retries = QemuOptions {
        attempts: 0,
        ..valid
    };
    let error = zero_retries.validate().unwrap_err();
    assert!(format!("{error:#}").contains("--retries 0"));

    let zero_smp = QemuOptions { smp: 0, ..valid };
    let error = zero_smp.validate().unwrap_err();
    assert!(format!("{error:#}").contains("--smp 0"));
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
    let root = env::temp_dir().join(format!(
        "xtask-prototyper-test-{}-{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
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

    remove_stale_generic_payload_artifacts(&root, "payload-test").unwrap();
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
    remove_stale_generic_payload_artifacts(&root, "payload").unwrap();
    assert!(root.join("rustsbi-prototyper-payload.elf").exists());
    remove_stale_generic_payload_artifacts(&root, "jump").unwrap();
    remove_stale_generic_payload_artifacts(&root, "payload-bench").unwrap();
    assert!(!root.join("rustsbi-prototyper-payload.elf").exists());
    let _ = fs::remove_dir_all(&root);
}
