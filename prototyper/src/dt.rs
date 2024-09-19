use serde::Deserialize;
use serde_device_tree::{buildin::StrSeq, Dtb, DtbPtr};

#[derive(Deserialize)]
pub struct Tree<'a> {
    pub model: Option<StrSeq<'a>>,
    pub chosen: Chosen<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Chosen<'a> {
    pub stdout_path: StrSeq<'a>,
}

pub enum ParseDeviceTreeError {
    Format,
}

pub fn parse_device_tree(opaque: usize) -> Result<Dtb, ParseDeviceTreeError> {
    let Ok(ptr) = DtbPtr::from_raw(opaque as *mut _) else {
        return Err(ParseDeviceTreeError::Format);
    };
    let dtb = Dtb::from(ptr);
    Ok(dtb)
}
