use serde::Deserialize;
use serde_device_tree::{buildin::StrSeq, from_raw_mut, Dtb, DtbPtr};

#[derive(Deserialize)]
pub struct Tree<'a> {
    pub chosen: Chosen<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Chosen<'a> {
    pub stdout_path: StrSeq<'a>,
}

pub enum ParseDeviceTreeError {
    Format,
    Deserialize,
}

pub fn parse_device_tree<'a>(opaque: usize) -> Result<Tree<'a>, ParseDeviceTreeError> {
    let Ok(ptr) = DtbPtr::from_raw(opaque as *mut _) else {
        return Err(ParseDeviceTreeError::Format);
    };
    let dtb = Dtb::from(ptr).share();
    let Ok(tree) = from_raw_mut(&dtb) else {
        return Err(ParseDeviceTreeError::Deserialize);
    };
    Ok(tree)
}
