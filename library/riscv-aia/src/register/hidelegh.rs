//! Upper 32 bits of Hypervisor interrupt delegation (hidelegh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of Hypervisor interrupt delegation.
    Hidelegh: 0x613,
    mask: 0xFFFF_FFFF,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidelegh_raw_roundtrip() {
        let bits = 0xDEAD_BEEFusize & 0xFFFF_FFFF;
        let reg = Hidelegh::from_bits(bits);
        assert_eq!(reg.bits(), bits);
    }
}
