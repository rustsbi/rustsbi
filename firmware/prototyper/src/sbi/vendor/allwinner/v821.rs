//! Allwinner V821 vendor SBI extensions.
//!
//! The extensions provide cache maintenance and optional USB DMA bypass.
//!
//! # References
//!
//! - Vendor ABI: [V821 U-Boot SBI definitions](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/u-boot-2018/arch/riscv/include/asm/sbi.h)
//!   — Andes extension ID and function IDs.
//! - Vendor caller: [V821 U-Boot A27L2 cache support](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/u-boot-2018/arch/riscv/cpu/A27L2/cache.c)
//!   — physical start/length range arguments used by the cache calls.

use crate::driver::platform::allwinner::v821::{
    a27l2::{self, A27L2Cache},
    usb_dma_bypass::{self, UsbDmaBypass},
};
use runtime::{
    Error, PlatformView, Result,
    memory::{MemoryRegistry, SupervisorMemory},
    rustsbi::{self, Extension as SbiExtension, SbiRet, VendorSBI},
    soc::allwinner::v821::{AllwinnerV821Soc, AndesStatusRegister},
};

/// EID of the V821 BSP's Andes cache-maintenance extension.
const EXTENSION: usize = 0x0900_031e;
/// EID of the V821 BSP's AWBASE extension.
const AWBASE_EXTENSION: usize = 0x5445_5335;

/// Vendor-specific extensions implemented by the V821 platform.
#[derive(VendorSBI)]
#[rustsbi(crate = rustsbi)]
pub(crate) struct Extensions {
    #[rustsbi(extension(eid = EXTENSION))]
    andes: Andes,
    #[rustsbi(extension(eid = AWBASE_EXTENSION))]
    awbase: Option<Awbase>,
}

impl Extensions {
    /// Initializes the V821 vendor extensions.
    ///
    /// Initialize the timer first so cache maintenance can time out while
    /// waiting for the controller.
    pub(crate) fn init(
        soc: AllwinnerV821Soc,
        platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
        supervisor_memory: &'static SupervisorMemory,
    ) -> Result<Self> {
        let cache = a27l2::describe(platform, soc)?.ok_or(Error::InvalidArgs)?;
        let usb_dma_bypass = usb_dma_bypass::describe(platform)?;
        let cache = A27L2Cache::init(cache, memory)?;
        let usb_dma_bypass = usb_dma_bypass
            .map(|description| UsbDmaBypass::init(description, memory))
            .transpose()?;
        Ok(Self {
            andes: Andes {
                cache,
                memory: supervisor_memory,
            },
            awbase: usb_dma_bypass.map(|device| Awbase { device }),
        })
    }
}

struct Andes {
    cache: A27L2Cache,
    memory: &'static SupervisorMemory,
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
            23 => sbi_result(self.cache.flush_all()),
            27 => sbi_result(self.cache.write_back_range(args[0], args[1], self.memory)),
            28 => sbi_result(self.cache.invalidate_range(args[0], args[1], self.memory)),
            _ => SbiRet::not_supported(),
        }
    }
}

struct Awbase {
    device: UsbDmaBypass,
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
