//! Allwinner V821 vendor SBI extensions.
//!
//! The V821-only cache and USB devices are owned by this platform; this module
//! translates their SBI function IDs into typed operations.
//!
//! # References
//!
//! - Vendor ABI: [V821 U-Boot SBI definitions](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/u-boot-2018/arch/riscv/include/asm/sbi.h)
//!   — Andes extension ID and function IDs.
//! - Vendor caller: [V821 U-Boot A27L2 cache support](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/u-boot-2018/arch/riscv/cpu/A27L2/cache.c)
//!   — physical start/length range arguments used by the cache calls.

use alloc::boxed::Box;

use super::V821State;
use super::devices::{A27L2Cache, UsbDmaBypass};
use runtime::{
    Error, Result,
    memory::{MemoryRegistry, SupervisorMemory},
    rustsbi::{self, Extension as SbiExtension, SbiRet, VendorSBI},
    soc::allwinner::v821::AndesStatusRegister,
};

/// EID of the V821 BSP's Andes cache-maintenance extension.
const EXTENSION: usize = 0x0900_031e;
/// EID of the V821 BSP's AWBASE extension.
const AWBASE_EXTENSION: usize = 0x5445_5335;

/// V821 custom-extension devices selected for this platform.
pub(super) struct Extension {
    cache: Option<A27L2Cache>,
    usb_dma_bypass: Option<UsbDmaBypass>,
}

impl Extension {
    pub(super) fn bind(v821: V821State, memory: &mut MemoryRegistry) -> Result<Self> {
        let (soc, cache, usb_dma_bypass) = v821.into_extension_devices()?.into_parts();
        Ok(Self {
            cache: Some(A27L2Cache::bind(soc, cache, memory)?),
            usb_dma_bypass: usb_dma_bypass
                .map(|registers| UsbDmaBypass::bind(registers, memory))
                .transpose()?,
        })
    }

    fn take_extensions(&mut self, memory: &'static SupervisorMemory) -> Extensions {
        Extensions {
            andes: Some(Andes::new(
                self.cache.take().expect("V821 cache extension bound twice"),
                memory,
            )),
            awbase: self.usb_dma_bypass.take().map(Awbase::new),
        }
    }
}

impl crate::sbi::vendor::VendorExtension for Extension {
    fn take_vendor(&mut self, memory: &'static SupervisorMemory) -> crate::sbi::vendor::Vendor {
        crate::sbi::vendor::Vendor::new(Box::new(self.take_extensions(memory)))
    }
}

/// Vendor-specific extensions implemented by the V821 platform.
#[derive(VendorSBI)]
#[rustsbi(crate = rustsbi)]
pub(crate) struct Extensions {
    #[rustsbi(extension(eid = EXTENSION))]
    andes: Option<Andes>,
    #[rustsbi(extension(eid = AWBASE_EXTENSION))]
    awbase: Option<Awbase>,
}

struct Andes {
    cache: A27L2Cache,
    memory: &'static SupervisorMemory,
}

impl Andes {
    fn new(cache: A27L2Cache, memory: &'static SupervisorMemory) -> Self {
        Self { cache, memory }
    }
}

impl SbiExtension for Andes {
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
            23 => sbi_result(self.cache.flush_all(crate::sbi::timer())),
            27 => sbi_result(self.cache.write_back_range(
                args[0],
                args[1],
                self.memory,
                crate::sbi::timer(),
            )),
            28 => sbi_result(self.cache.invalidate_range(
                args[0],
                args[1],
                self.memory,
                crate::sbi::timer(),
            )),
            _ => SbiRet::not_supported(),
        }
    }
}

struct Awbase {
    device: UsbDmaBypass,
}

impl Awbase {
    fn new(device: UsbDmaBypass) -> Self {
        Self { device }
    }
}

impl SbiExtension for Awbase {
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

fn sbi_result(result: Result<()>) -> SbiRet {
    match result {
        Ok(()) => SbiRet::success(0),
        Err(Error::InvalidArgs | Error::Overflow) => SbiRet::invalid_param(),
        Err(_) => SbiRet::failed(),
    }
}
