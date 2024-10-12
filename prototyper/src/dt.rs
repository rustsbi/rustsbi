use serde::Deserialize;
use serde_device_tree::{
    buildin::{NodeSeq, StrSeq},
    Dtb, DtbPtr,
};

#[derive(Deserialize)]
pub struct Tree<'a> {
    pub model: Option<StrSeq<'a>>,
    pub chosen: Chosen<'a>,
    pub cpus: Cpus<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Chosen<'a> {
    pub stdout_path: StrSeq<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Cpus<'a> {
    pub cpu: NodeSeq<'a>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct Cpu<'a> {
    #[serde(rename = "riscv,isa-extensions")]
    pub isa: StrSeq<'a>,
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
