//! Safe accessors for the V821 BSP's Andes custom CSRs.
//!
//! V821 pairs Allwinner platform devices with Andes cores and an A27L2 cache.
//! Its BSP therefore uses Andes custom CSRs for PMA configuration, machine
//! status, and cache-line commands. These CSR numbers and command encodings are
//! isolated here; callers must present a V821 capability to use them.
//! The API remains visible on non-RISC-V targets for static analysis, but an
//! attempted hardware operation on such a target panics.
//!
//! # References
//!
//! - Vendor implementation: [V821 SPL cache support](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/spl/arch/riscv/cpu/ads_rv32/mmu.c)
//!   — cache-control CSR numbers and command encodings.

use super::AllwinnerV821Soc;

const NONCACHEABLE_ALIAS_OFFSET: u64 = 0x1_0000_0000;

/// An Andes machine-control status register exposed by the vendor SBI ABI.
#[derive(Clone, Copy)]
pub enum AndesStatusRegister {
    /// Machine cache-control status (`MCACHE_CTL`).
    MachineCacheControl,
    /// Machine miscellaneous-control status (`MMISC_CTL`).
    MachineMiscControl,
}

/// An Andes custom cache command accepted by the V821 BSP.
#[derive(Clone, Copy)]
pub enum A27L2LineOperation {
    /// Invalidate an L1 data-cache line.
    Invalidate,
    /// Write back a dirty L1 line.
    WriteBack,
    /// Write back and invalidate an L1 line.
    WriteBackAndInvalidate,
}

impl A27L2LineOperation {
    const fn encoding(self) -> usize {
        match self {
            Self::Invalidate => 0,
            Self::WriteBack => 1,
            Self::WriteBackAndInvalidate => 2,
        }
    }
}

/// The noncacheable physical alias established by the V821 PMA setup.
///
/// Values of this type are returned only after the custom PMA registers have
/// been programmed. Firmware policy can therefore use the offset without
/// duplicating the V821 register contract.
#[derive(Clone, Copy)]
#[must_use = "the alias offset is required to protect firmware memory in PMP"]
pub struct NoncacheableAlias {
    offset: u64,
}

impl NoncacheableAlias {
    /// Returns the offset from a cacheable physical address to its alias.
    pub const fn offset(self) -> u64 {
        self.offset
    }
}

/// Access to the Andes custom CSRs used by the V821 BSP.
///
/// This zero-sized capability can only be obtained from a Devicetree-selected
/// [`AllwinnerV821Soc`]. It keeps raw CSR numbers and encodings inside Runtime.
#[derive(Clone, Copy)]
pub struct V821Csr {
    _private: (),
}

impl AllwinnerV821Soc {
    /// Returns the custom-CSR interface authorized by this SoC description.
    pub const fn csr(self) -> V821Csr {
        V821Csr { _private: () }
    }
}

impl V821Csr {
    /// Configures the BSP-reserved PMA entry as the 4--8 GiB uncached alias.
    pub fn initialize_noncacheable_alias(self) -> NoncacheableAlias {
        arch::initialize_noncacheable_alias();
        NoncacheableAlias {
            offset: NONCACHEABLE_ALIAS_OFFSET,
        }
    }

    /// Reads the selected Andes machine-control status register.
    pub fn read_status(self, register: AndesStatusRegister) -> usize {
        arch::read_status(register)
    }

    /// Applies one cache-maintenance operation to a physical cache line.
    pub fn maintain_a27l2_line(self, address: usize, operation: A27L2LineOperation) {
        arch::maintain_a27l2_line(address, operation.encoding());
    }

    /// Writes back and invalidates all A27L2 L1 data-cache lines.
    pub fn write_back_and_invalidate_l1_all(self) {
        arch::write_back_and_invalidate_l1_all();
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod arch {
    use super::AndesStatusRegister;

    const PMA_CONFIG_CSR: usize = 0xbc3;
    const PMA_BOUNDARY_CSR: usize = 0xbdf;
    const MACHINE_CACHE_CONTROL_CSR: usize = 0x7ca;
    const MACHINE_MISC_CONTROL_CSR: usize = 0x7d0;
    const CACHE_ADDRESS_CSR: usize = 0x80b;
    const CACHE_COMMAND_CSR: usize = 0x80c;

    const PMA_BOUNDARY_VALUE: usize = 0x5fff_ffff;
    const PMA_CONFIG_MASK: usize = 0xff00_0000;
    const PMA_CONFIG_VALUE: usize = 0x0f00_0000;
    const WRITE_BACK_AND_INVALIDATE_ALL: usize = 6;

    pub(super) fn initialize_noncacheable_alias() {
        // SAFETY: the public interface requires a V821 capability. The boot
        // path invokes this in M-mode and preserves PMA fields it does not own.
        unsafe {
            let config: usize;
            core::arch::asm!(
                "csrr {value}, {config_csr}",
                value = out(reg) config,
                config_csr = const PMA_CONFIG_CSR,
                options(nomem, nostack),
            );
            core::arch::asm!(
                "csrw {config_csr}, {config}", "csrw {bound_csr}, {boundary}", "fence",
                config_csr = const PMA_CONFIG_CSR,
                bound_csr = const PMA_BOUNDARY_CSR,
                boundary = in(reg) PMA_BOUNDARY_VALUE,
                config = in(reg) (config & !PMA_CONFIG_MASK) | PMA_CONFIG_VALUE,
                options(nostack),
            );
        }
    }

    pub(super) fn read_status(register: AndesStatusRegister) -> usize {
        let value;
        // SAFETY: the public interface requires a V821 capability, and this is
        // a plain M-mode CSR read on the calling hart.
        unsafe {
            match register {
                AndesStatusRegister::MachineCacheControl => core::arch::asm!(
                    "csrr {value}, {csr}",
                    value = out(reg) value,
                    csr = const MACHINE_CACHE_CONTROL_CSR,
                    options(nomem, nostack),
                ),
                AndesStatusRegister::MachineMiscControl => core::arch::asm!(
                    "csrr {value}, {csr}",
                    value = out(reg) value,
                    csr = const MACHINE_MISC_CONTROL_CSR,
                    options(nomem, nostack),
                ),
            }
        }
        value
    }

    pub(super) fn maintain_a27l2_line(address: usize, command: usize) {
        // SAFETY: the public enum constrains the command encoding. Hardware
        // receives the address as data; Rust does not dereference it.
        unsafe {
            core::arch::asm!(
                "fence", "csrw {address_csr}, {address}",
                "csrw {command_csr}, {command}",
                address_csr = const CACHE_ADDRESS_CSR,
                command_csr = const CACHE_COMMAND_CSR,
                address = in(reg) address,
                command = in(reg) command,
                options(nostack),
            );
        }
    }

    pub(super) fn write_back_and_invalidate_l1_all() {
        // SAFETY: command 6 is the V821 BSP's all-lines
        // writeback-and-invalidate operation.
        unsafe {
            core::arch::asm!(
                "fence", "csrw {command_csr}, {command}",
                command_csr = const CACHE_COMMAND_CSR,
                command = in(reg) WRITE_BACK_AND_INVALIDATE_ALL,
                options(nostack),
            );
        }
    }
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod arch {
    use super::AndesStatusRegister;

    pub(super) fn initialize_noncacheable_alias() {
        unsupported()
    }

    pub(super) fn read_status(_register: AndesStatusRegister) -> usize {
        unsupported()
    }

    pub(super) fn maintain_a27l2_line(_address: usize, _command: usize) {
        unsupported()
    }

    pub(super) fn write_back_and_invalidate_l1_all() {
        unsupported()
    }

    #[cold]
    #[track_caller]
    fn unsupported() -> ! {
        panic!("V821 custom CSRs require a RISC-V target")
    }
}
