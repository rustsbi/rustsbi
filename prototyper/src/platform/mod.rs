#[cfg(not(feature = "payload"))]
pub mod dynamic;
#[cfg(feature = "payload")]
pub mod payload;

use riscv::register::mstatus;

pub struct BootInfo {
    pub next_address: usize,
    pub fdt_address: usize,
    pub is_boot_hart: bool,
    pub mpp: mstatus::MPP,
}

#[cfg(not(feature = "payload"))]
pub use dynamic::get_boot_info;
#[cfg(feature = "payload")]
pub use payload::get_boot_info;
