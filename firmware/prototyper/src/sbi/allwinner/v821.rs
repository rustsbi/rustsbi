//! V821 custom SBI extension adapters.
//!
//! [`Extensions`] exposes only extensions backed by devices acquired through
//! the V821 platform description. Device ownership remains in the firmware
//! drivers; this module translates SBI function IDs into typed operations.
//!
//! # References
//!
//! - Vendor ABI: [V821 U-Boot SBI definitions](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/u-boot-2018/arch/riscv/include/asm/sbi.h)
//!   — Andes extension ID and function IDs.
//! - Vendor caller: [V821 U-Boot A27L2 cache support](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/u-boot-2018/arch/riscv/cpu/A27L2/cache.c)
//!   — physical start/length range arguments used by the cache calls.

use crate::driver::allwinner::v821::{A27L2Cache, UsbDmaBypass};
use crate::platform::allwinner::v821::V821;
use runtime::memory::MemoryRegistry;
use runtime::memory::SupervisorMemory;
use runtime::rustsbi::{Extension, SbiRet};
use runtime::soc::allwinner::v821::AndesStatusRegister;

/// EID of the V821 BSP's Andes cache-maintenance extension.
pub(crate) const EXTENSION: usize = 0x0900_031e;
/// EID of the V821 BSP's AWBASE extension.
pub(crate) const AWBASE_EXTENSION: usize = 0x5445_5335;

/// Bound V821 devices waiting for supervisor-memory publication.
pub(super) struct BoundExtensions {
    cache: A27L2Cache,
    usb_dma_bypass: Option<UsbDmaBypass>,
}

impl BoundExtensions {
    pub(super) fn bind(v821: V821, memory: &mut MemoryRegistry) -> runtime::Result<Self> {
        let (soc, cache, usb_dma_bypass) = v821.into_extension_devices()?.into_parts();
        Ok(Self {
            cache: A27L2Cache::bind(soc, cache, memory)?,
            usb_dma_bypass: usb_dma_bypass
                .map(|registers| UsbDmaBypass::bind(registers, memory))
                .transpose()?,
        })
    }

    pub(super) fn into_extensions(self, memory: &'static SupervisorMemory) -> Extensions {
        Extensions {
            andes: Some(Andes::new(self.cache, memory)),
            awbase: self.usb_dma_bypass.map(Awbase::new),
        }
    }
}

/// Custom SBI extensions constructed from the V821-owned devices.
#[derive(Default)]
pub(crate) struct Extensions {
    pub(crate) andes: Option<Andes>,
    pub(crate) awbase: Option<Awbase>,
}

pub(crate) struct Andes {
    cache: A27L2Cache,
    memory: &'static SupervisorMemory,
}

impl Andes {
    fn new(cache: A27L2Cache, memory: &'static SupervisorMemory) -> Self {
        Self { cache, memory }
    }
}

impl Extension for Andes {
    #[inline]
    fn probe(&self) -> usize {
        1
    }

    fn handle(&self, fid: usize, args: [usize; 6]) -> SbiRet {
        match fid {
            0 => SbiRet::success(
                self.cache
                    .read_status(AndesStatusRegister::MachineCacheControl),
            ),
            1 => SbiRet::success(
                self.cache
                    .read_status(AndesStatusRegister::MachineMiscControl),
            ),
            23 => sbi_result(self.cache.flush_all()),
            27 => sbi_result(self.cache.write_back_range(args[0], args[1], self.memory)),
            28 => sbi_result(self.cache.invalidate_range(args[0], args[1], self.memory)),
            _ => SbiRet::not_supported(),
        }
    }
}

pub(crate) struct Awbase {
    device: UsbDmaBypass,
}

impl Awbase {
    fn new(device: UsbDmaBypass) -> Self {
        Self { device }
    }
}

impl Extension for Awbase {
    #[inline]
    fn probe(&self) -> usize {
        1
    }

    fn handle(&self, fid: usize, _: [usize; 6]) -> SbiRet {
        match fid {
            11 => sbi_result(self.device.enable()),
            _ => SbiRet::not_supported(),
        }
    }
}

fn sbi_result(result: runtime::Result<()>) -> SbiRet {
    match result {
        Ok(()) => SbiRet::success(0),
        Err(runtime::Error::InvalidArgs | runtime::Error::Overflow) => SbiRet::invalid_param(),
        Err(_) => SbiRet::failed(),
    }
}
