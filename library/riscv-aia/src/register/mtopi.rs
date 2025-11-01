//! Machine-level top interrupt register.
//!
//! CSR `mtopi` reports the highest-priority interrupt that is pending and enabled for machine level.
use crate::Iid;

riscv::read_only_csr! {
    /// Machine top interrupt register.
    Mtopi: 0xFB0,
    mask: 0x0FFF_00FF,
}

impl Mtopi {
    /// Get the major identity number of the highest-priority interrupt.
    #[inline]
    pub const fn iid(self) -> Option<Iid> {
        let bits = (self.bits & 0x0FFF_0000) >> 16;
        Iid::new(bits as u16)
    }

    /// Indicates the priority number of the highest-priority interrupt.
    #[inline]
    pub const fn iprio(self) -> u8 {
        (self.bits & 0x0000_00FF) as u8
    }
}

// TODO test module of Mtopi and Iid structures.
