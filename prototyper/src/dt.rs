use serde::Deserialize;
use serde_device_tree::{
    buildin::{NodeSeq, Reg, StrSeq},
    Dtb, DtbPtr,
};

#[derive(Deserialize)]
pub struct Tree<'a> {
    pub model: Option<StrSeq<'a>>,
    pub chosen: Chosen<'a>,
    pub cpus: Cpus<'a>,
    pub soc: Soc<'a>,
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

#[derive(Deserialize, Debug)]
pub struct Cpu<'a> {
    #[serde(rename = "riscv,isa-extensions")]
    pub isa: Option<StrSeq<'a>>,
}

#[derive(Deserialize, Debug)]
pub struct Soc<'a> {
    pub serial: Option<NodeSeq<'a>>,
    pub test: Option<NodeSeq<'a>>,
    pub clint: Option<NodeSeq<'a>>,
}

#[allow(unused)]
#[derive(Deserialize, Debug)]
pub struct Device<'a> {
    pub reg: Reg<'a>,
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
