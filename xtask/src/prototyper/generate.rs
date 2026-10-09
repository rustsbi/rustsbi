use std::{
    fs,
    hash::{Hash, Hasher},
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::utils::{cargo_target_dir, workspace_root};

use super::{build::BuildMode, config::BuildSpec};

const CONFIG_FILE_NAME: &str = "config.toml";
const BUILD_INPUTS_DIR_NAME: &str = "prototyper";
const LINKER_SCRIPT_NAME: &str = "rustsbi-prototyper.ld";
const CONFIG_SOURCE_NAME: &str = "generated_config.rs";
const PAYLOAD_DATA_NAME: &str = "payload.bin";
const FDT_DATA_NAME: &str = "fdt.bin";
const STAMP_FILE_NAME: &str = "stamp";

/// Workspace paths used by one prototyper build.
#[derive(Debug)]
pub(crate) struct BuildPaths {
    pub(crate) target_dir: PathBuf,
    pub(crate) artifact_dir: PathBuf,
    pub(crate) build_inputs_dir: PathBuf,
    pub(crate) linker_template: PathBuf,
}

impl BuildPaths {
    pub(crate) fn config_source(&self) -> PathBuf {
        self.build_inputs_dir.join(CONFIG_SOURCE_NAME)
    }

    pub(crate) fn linker_script(&self) -> PathBuf {
        self.build_inputs_dir.join(LINKER_SCRIPT_NAME)
    }

    pub(crate) fn payload_data(&self) -> PathBuf {
        self.build_inputs_dir.join(PAYLOAD_DATA_NAME)
    }

    pub(crate) fn payload_object(&self) -> PathBuf {
        self.build_inputs_dir.join("payload.o")
    }

    pub(crate) fn fdt_data(&self) -> PathBuf {
        self.build_inputs_dir.join(FDT_DATA_NAME)
    }

    pub(crate) fn fdt_object(&self) -> PathBuf {
        self.build_inputs_dir.join("fdt.o")
    }

    pub(crate) fn stamp(&self) -> PathBuf {
        self.build_inputs_dir.join(STAMP_FILE_NAME)
    }
}

pub(crate) fn prepare_build_paths(spec: &BuildSpec) -> Result<BuildPaths> {
    let workspace_root = workspace_root();
    let target_dir = cargo_target_dir();
    let artifact_dir = spec.artifact_dir_in(&target_dir);
    let build_inputs_dir = target_dir.join(BUILD_INPUTS_DIR_NAME);
    let linker_template = workspace_root
        .join("firmware")
        .join("prototyper")
        .join("rustsbi-prototyper.ld.in");

    Ok(BuildPaths {
        target_dir,
        artifact_dir,
        build_inputs_dir,
        linker_template,
    })
}

/// Installs the files consumed by the firmware crate for this build.
pub(crate) fn generate_build_inputs(spec: &BuildSpec, paths: &BuildPaths) -> Result<()> {
    fs::create_dir_all(&paths.build_inputs_dir).with_context(|| {
        format!(
            "failed to prepare build inputs: cannot create directory '{}'",
            paths.build_inputs_dir.display()
        )
    })?;

    info!(
        "Copy config from: {}",
        spec.firmware_config.source.display()
    );
    let config_content = spec.firmware_config.content.as_bytes();
    write_if_changed(
        &paths.build_inputs_dir.join(CONFIG_FILE_NAME),
        config_content,
    )?;

    let config_source = render_config_source(
        &paths.build_inputs_dir.join(CONFIG_FILE_NAME),
        &spec.firmware_config.layout,
    )?;
    write_if_changed(&paths.config_source(), config_source.as_bytes())?;

    let linker_template = fs::read_to_string(&paths.linker_template).with_context(|| {
        format!(
            "failed to read linker script template '{}'",
            paths.linker_template.display()
        )
    })?;
    let linker_script = render_linker_script(
        &linker_template,
        &spec.firmware_config.layout,
        match spec.mode {
            BuildMode::Payload { .. } => Some(spec.firmware_config.layout.payload_address),
            BuildMode::Dynamic | BuildMode::Jump => None,
        },
    )?;
    write_if_changed(&paths.linker_script(), linker_script.as_bytes())?;

    let payload = match &spec.mode {
        BuildMode::Payload { path } => Some(read_image(path)?),
        BuildMode::Dynamic | BuildMode::Jump => None,
    };
    install_image(
        payload.as_deref(),
        &paths.payload_data(),
        &paths.payload_object(),
    )?;
    let fdt = spec.fdt.as_deref().map(read_image).transpose()?;
    install_image(fdt.as_deref(), &paths.fdt_data(), &paths.fdt_object())?;

    let stamp = render_build_stamp(
        spec,
        config_content,
        &config_source,
        &linker_template,
        payload.as_deref(),
        fdt.as_deref(),
    );
    write_if_changed(&paths.stamp(), stamp.as_bytes())?;

    Ok(())
}

fn render_config_source(
    config_file: &Path,
    layout: &super::config::FirmwareLayout,
) -> Result<String> {
    let config_file = config_file
        .to_str()
        .with_context(|| format!("config path '{}' is not valid UTF-8", config_file.display()))?;
    Ok(format!(
        "static_toml! {{ const CONFIG = include_toml!({config_file:?}); }}\n\
         pub(crate) const HART_CAPACITY: usize = {};\n\
         pub(crate) const STACK_SIZE_PER_HART: usize = {};\n",
        layout.hart_capacity, layout.stack_size_per_hart
    ))
}

fn read_image(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read embedded image '{}'", path.display()))?;
    if bytes.is_empty() {
        bail!("embedded image '{}' is empty", path.display());
    }
    Ok(bytes)
}

fn install_image(bytes: Option<&[u8]>, data: &Path, object: &Path) -> Result<()> {
    if let Some(bytes) = bytes {
        write_if_changed(data, bytes)
    } else {
        remove_generated_file(data)?;
        remove_generated_file(object)
    }
}

fn remove_generated_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("failed to remove generated image '{}'", path.display())),
    }
}

fn render_build_stamp(
    spec: &BuildSpec,
    config_content: &[u8],
    config_source: &str,
    linker_template: &str,
    payload: Option<&[u8]>,
    fdt: Option<&[u8]>,
) -> String {
    let mode = match &spec.mode {
        BuildMode::Dynamic => "dynamic".to_string(),
        BuildMode::Jump => "jump".to_string(),
        BuildMode::Payload { path } => format!("payload {}", path.display()),
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    mode.hash(&mut hasher);
    spec.cargo_features().hash(&mut hasher);
    spec.no_default_features.hash(&mut hasher);
    spec.target.triple().hash(&mut hasher);
    spec.custom_target.hash(&mut hasher);
    spec.profile().hash(&mut hasher);
    config_content.hash(&mut hasher);
    config_source.hash(&mut hasher);
    linker_template.hash(&mut hasher);
    payload.hash(&mut hasher);
    fdt.hash(&mut hasher);
    format!("{:016x}\n", hasher.finish())
}

/// Renders the linker script from its firmware layout template.
pub(crate) fn render_linker_script(
    template: &str,
    layout: &super::config::FirmwareLayout,
    payload_address: Option<u64>,
) -> Result<String> {
    // An empty output section must not move the location counter backwards.
    // Some boards place the unused payload address below the debug image end.
    let payload_address =
        payload_address.map_or_else(String::new, |address| format!("{address:#x}"));
    let rendered = template
        .replace(
            "@LINK_START_ADDRESS@",
            &format!("{:#x}", layout.link_start_address),
        )
        .replace("@HEAP_SIZE@", &format!("{:#x}", layout.heap_size_bytes))
        .replace(
            "@STACK_SIZE_PER_HART@",
            &format!("{:#x}", layout.stack_size_per_hart),
        )
        .replace("@PAYLOAD_ADDRESS@", &payload_address);
    reject_unknown_placeholders(&rendered)?;
    Ok(rendered)
}

/// Rejects unresolved `@TOKEN@` placeholders in the rendered linker script.
///
/// Tokens are nonempty and contain only uppercase letters, digits, and
/// underscores. Other uses of `@`, including those in comments, are allowed.
fn reject_unknown_placeholders(rendered: &str) -> Result<()> {
    let mut rest = rendered;
    while let Some(open) = rest.find('@') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('@') else {
            break;
        };
        let token = &after_open[..close];
        if !token.is_empty()
            && token
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            bail!("linker script template contains an unknown `@{token}@` placeholder");
        }
        rest = &after_open[close + 1..];
    }
    Ok(())
}

/// Writes `content` only when it differs from the existing file.
pub(super) fn write_if_changed(path: &Path, content: &[u8]) -> Result<()> {
    if fs::read(path).is_ok_and(|existing| existing == content) {
        return Ok(());
    }
    info!("Writing generated file: {}", path.display());
    fs::write(path, content)
        .with_context(|| format!("failed to write generated file '{}'", path.display()))
}
