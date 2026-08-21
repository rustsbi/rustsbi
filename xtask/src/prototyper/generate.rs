use std::{
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::utils::workspace_root;

use super::{build::BuildMode, config::BuildSpec};

const CONFIG_FILE_NAME: &str = "config.toml";
const BUILD_INPUTS_DIR_NAME: &str = "target/prototyper";
const LINKER_SCRIPT_NAME: &str = "rustsbi-prototyper.ld";
const ALIGNMENT_SOURCE_NAME: &str = "generated_alignment.rs";
const PAYLOAD_SOURCE_NAME: &str = "generated_payload.rs";
const FDT_SOURCE_NAME: &str = "generated_fdt.rs";
const STAMP_FILE_NAME: &str = "stamp";

/// Workspace paths used by one prototyper build.
#[derive(Debug)]
pub(crate) struct BuildPaths {
    pub(crate) artifact_dir: PathBuf,
    pub(crate) build_inputs_dir: PathBuf,
    pub(crate) linker_template: PathBuf,
}

impl BuildPaths {
    pub(crate) fn linker_script(&self) -> PathBuf {
        self.build_inputs_dir.join(LINKER_SCRIPT_NAME)
    }

    /// Absolute linker script path for the cargo `-T` link argument.
    pub(crate) fn linker_script_argument(&self) -> PathBuf {
        self.linker_script()
    }

    pub(crate) fn alignment_source(&self) -> PathBuf {
        self.build_inputs_dir.join(ALIGNMENT_SOURCE_NAME)
    }

    pub(crate) fn payload_source(&self) -> PathBuf {
        self.build_inputs_dir.join(PAYLOAD_SOURCE_NAME)
    }

    pub(crate) fn fdt_source(&self) -> PathBuf {
        self.build_inputs_dir.join(FDT_SOURCE_NAME)
    }

    pub(crate) fn stamp(&self) -> PathBuf {
        self.build_inputs_dir.join(STAMP_FILE_NAME)
    }
}

pub(crate) fn prepare_build_paths(spec: &BuildSpec) -> Result<BuildPaths> {
    let workspace_root = workspace_root();
    let artifact_dir = spec.artifact_dir();
    let build_inputs_dir = workspace_root.join(BUILD_INPUTS_DIR_NAME);
    let linker_template = workspace_root
        .join("prototyper")
        .join("prototyper")
        .join("rustsbi-prototyper.ld.in");

    Ok(BuildPaths {
        artifact_dir,
        build_inputs_dir,
        linker_template,
    })
}

/// Install the files consumed by the firmware crate for this build.
pub(crate) fn generate_build_inputs(spec: &BuildSpec, paths: &BuildPaths) -> Result<()> {
    fs::create_dir_all(&paths.build_inputs_dir).with_context(|| {
        format!(
            "failed to prepare build inputs: cannot create directory '{}'",
            paths.build_inputs_dir.display()
        )
    })?;

    info!("Copy config from: {}", spec.config_source.display());
    let config_content = fs::read(&spec.config_source).with_context(|| {
        format!(
            "failed to read config file '{}'",
            spec.config_source.display()
        )
    })?;
    write_if_changed(
        &paths.build_inputs_dir.join(CONFIG_FILE_NAME),
        &config_content,
    )?;

    let linker_template = fs::read_to_string(&paths.linker_template).with_context(|| {
        format!(
            "failed to read linker script template '{}'",
            paths.linker_template.display()
        )
    })?;
    let linker_script = render_linker_script(&linker_template, &spec.platform_addresses)?;
    write_if_changed(&paths.linker_script(), linker_script.as_bytes())?;

    let alignment_source = render_alignment_source();
    write_if_changed(&paths.alignment_source(), alignment_source.as_bytes())?;

    let payload_source = render_payload_source(spec)?;
    write_if_changed(&paths.payload_source(), payload_source.as_bytes())?;

    let fdt_source = render_fdt_source(spec)?;
    write_if_changed(&paths.fdt_source(), fdt_source.as_bytes())?;

    let stamp = render_build_stamp(
        spec,
        &config_content,
        &linker_template,
        &alignment_source,
        &payload_source,
        &fdt_source,
    );
    write_if_changed(&paths.stamp(), stamp.as_bytes())?;

    Ok(())
}

fn render_alignment_source() -> String {
    String::from(
        "#[allow(dead_code)]\n\
         #[repr(align(4))]\n\
         pub struct Aligned4<const N: usize>(pub [u8; N]);\n\
         #[allow(dead_code)]\n\
         #[repr(align(16))]\n\
         pub struct Aligned16<const N: usize>(pub [u8; N]);\n",
    )
}

fn render_payload_source(spec: &BuildSpec) -> Result<String> {
    match &spec.mode {
        BuildMode::Payload { path } => {
            render_embedded_static("payload_image", ".payload", "Aligned4", path)
        }
        BuildMode::Dynamic | BuildMode::Jump => Ok(String::new()),
    }
}

fn render_fdt_source(spec: &BuildSpec) -> Result<String> {
    match &spec.fdt {
        Some(path) => render_embedded_static("raw_fdt", ".fdt", "Aligned16", path),
        None => Ok(String::new()),
    }
}

/// Render one embedded binary static.
fn render_embedded_static(
    symbol_name: &str,
    section_name: &str,
    alignment_type: &str,
    path: &Path,
) -> Result<String> {
    let size = fs::metadata(path)
        .with_context(|| {
            format!(
                "failed to generate embedded firmware source: cannot read '{}'",
                path.display()
            )
        })?
        .len();
    let path_string = path
        .to_str()
        .with_context(|| format!("path '{}' is not valid UTF-8", path.display()))?;
    Ok(format!(
        "\n#[allow(dead_code, non_upper_case_globals)]\n\
         #[unsafe(link_section = \"{section_name}\")]\n\
         pub static {symbol_name}: {alignment_type}<{size}> = {alignment_type}(*include_bytes!({path_string:?}));\n"
    ))
}

fn render_build_stamp(
    spec: &BuildSpec,
    config_content: &[u8],
    linker_template: &str,
    alignment_source: &str,
    payload_source: &str,
    fdt_source: &str,
) -> String {
    let mode = match &spec.mode {
        BuildMode::Dynamic => "dynamic".to_string(),
        BuildMode::Jump => "jump".to_string(),
        BuildMode::Payload { path } => format!("payload {}", path.display()),
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    mode.hash(&mut hasher);
    spec.cargo_features().hash(&mut hasher);
    spec.target.hash(&mut hasher);
    spec.profile().hash(&mut hasher);
    config_content.hash(&mut hasher);
    linker_template.hash(&mut hasher);
    alignment_source.hash(&mut hasher);
    payload_source.hash(&mut hasher);
    fdt_source.hash(&mut hasher);
    format!("{:016x}\n", hasher.finish())
}

/// Render the linker script from its address template.
pub(crate) fn render_linker_script(
    template: &str,
    addresses: &super::config::PlatformAddresses,
) -> Result<String> {
    let rendered = template
        .replace(
            "@LINK_START_ADDRESS@",
            &format!("{:#x}", addresses.link_start_address),
        )
        .replace(
            "@PAYLOAD_ADDRESS@",
            &format!("{:#x}", addresses.payload_address),
        );
    if rendered.contains('@') {
        bail!("linker script template contains an unknown `@TOKEN@` placeholder");
    }
    Ok(rendered)
}

/// Write `content` only when it differs from the existing file.
fn write_if_changed(path: &Path, content: &[u8]) -> Result<()> {
    if fs::read(path).is_ok_and(|existing| existing == content) {
        return Ok(());
    }
    info!("Writing generated file: {}", path.display());
    fs::write(path, content)
        .with_context(|| format!("failed to write generated file '{}'", path.display()))
}
