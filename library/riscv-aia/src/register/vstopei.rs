//! Virtual supervisor top external interrupt (only with an IMSIC) (vstopei)

use crate::Iid;

riscv::read_only_csr! {
	/// Virtual supervisor top external interrupt register.
	Vstopei: 0x25C,
	mask: 0x0FFF_00FF,
}

impl Vstopei {
	/// Get the major identity number of the highest-priority external interrupt.
	#[inline]
	pub const fn iid(self) -> Option<Iid> {
		let bits = (self.bits & 0x0FFF_0000) >> 16;
		Iid::new(bits as u16)
	}

	/// Indicates the priority number of the highest-priority external interrupt.
	#[inline]
	pub const fn iprio(self) -> u8 {
		(self.bits & 0x0000_00FF) as u8
	}
}
