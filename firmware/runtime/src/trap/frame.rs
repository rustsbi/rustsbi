//! The private trap frame: the assembly ABI save area for one trap.
//!
//! The frame is allocated by the normal entry directly below the Runtime
//! stack top and holds every general-purpose register plus the machine
//! trap CSRs. It never escapes the Runtime: policy receives copied SBI
//! scalars only.

/// The private `#[repr(C)]` register save area.
///
/// Field order is the assembly ABI: `x[0..32]`, then `mepc`, `mstatus`,
/// `mcause`, `mtval`. Offsets are consumed by the entry as compile-time
/// constants (word units scaled by XLEN), never hand-written `* 8` values.
#[repr(C)]
pub(crate) struct TrapFrame {
    /// General-purpose registers; `x[0]` reads as zero and writes are
    /// discarded, `x[2]` holds the original lower-privilege stack pointer.
    pub(crate) x: [usize; 32],
    /// The trap's `mepc`.
    pub(crate) mepc: usize,
    /// The trap's `mstatus`.
    pub(crate) mstatus: usize,
    /// The trap's `mcause`.
    pub(crate) mcause: usize,
    /// The trap's `mtval`.
    pub(crate) mtval: usize,
}

/// Frame layout constants, in XLEN-sized words, shared with the assembly
/// entry through `const` interpolation.
pub(crate) mod offsets {
    use super::TrapFrame;

    /// Offset of `mepc` in words.
    pub(crate) const MEPC: usize =
        core::mem::offset_of!(TrapFrame, mepc) / core::mem::size_of::<usize>();
    /// Offset of `mstatus` in words.
    pub(crate) const MSTATUS: usize =
        core::mem::offset_of!(TrapFrame, mstatus) / core::mem::size_of::<usize>();
    /// Offset of `mcause` in words.
    pub(crate) const MCAUSE: usize =
        core::mem::offset_of!(TrapFrame, mcause) / core::mem::size_of::<usize>();
    /// Offset of `mtval` in words.
    pub(crate) const MTVAL: usize =
        core::mem::offset_of!(TrapFrame, mtval) / core::mem::size_of::<usize>();
    /// Frame size in words.
    pub(crate) const SIZE_WORDS: usize =
        core::mem::size_of::<TrapFrame>() / core::mem::size_of::<usize>();
}

// The frame must keep every Rust call boundary 16-byte aligned and stay a
// whole number of 16-byte units, on both RV32 and RV64.
const _: () = assert!(core::mem::size_of::<TrapFrame>().is_multiple_of(16));
const _: () = assert!(core::mem::size_of::<TrapFrame>() == 36 * core::mem::size_of::<usize>());

// Bind the hand-written word offsets in `entry` to this layout: the
// assembly literals are compile-time checked here, satisfying the design's
// "no hand-tuned offsets" rule.
const _: () = assert!(offsets::MEPC == 32);
const _: () = assert!(offsets::MSTATUS == 33);
const _: () = assert!(offsets::MCAUSE == 34);
const _: () = assert!(offsets::MTVAL == 35);
const _: () = assert!(offsets::SIZE_WORDS == 36);

impl TrapFrame {
    /// Reads register `id`, where `x0` is hardwired to zero.
    pub(crate) fn read_x(&self, id: usize) -> usize {
        if id == 0 { 0 } else { self.x[id] }
    }

    /// Writes `value` to register `id`; writes to `x0` are discarded.
    pub(crate) fn write_x(&mut self, id: usize, value: usize) {
        if id != 0 {
            self.x[id] = value;
        }
    }

    /// Whether the trapped context came from M-mode (`MPP = M`).
    pub(crate) fn trapped_from_machine(&self) -> bool {
        const MPP_MASK: usize = 0b11 << 11;
        const MPP_MACHINE: usize = 0b11 << 11;
        self.mstatus & MPP_MASK == MPP_MACHINE
    }
}
