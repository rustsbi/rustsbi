//! Guest external interrupt enable and pending sets.

csr_value! {
    /// Guest external interrupt sources indexed by guest interrupt number.
    ///
    /// Bits 1 through `GEILEN` identify this hart's guest external interrupts.
    /// Bit zero and bits above `GEILEN` read as zero in both CSRs.
    pub struct GuestExternalInterruptSet;
}

define_csr! {
    /// Hypervisor guest external interrupt-enable register (`hgeie`).
    ///
    /// # Access requirements
    ///
    /// Coordinate guest external interrupt files with hypervisor handlers.
    Hgeie { 0x607, r: GuestExternalInterruptSet, w: GuestExternalInterruptSet };

    /// Hypervisor guest external interrupt-pending register (`hgeip`, read-only).
    Hgeip { 0xe12, r: GuestExternalInterruptSet };
}
