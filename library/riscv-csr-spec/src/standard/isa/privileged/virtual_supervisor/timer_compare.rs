//! Virtual-supervisor timer comparator.

define_csr! {
    /// Virtual supervisor timer comparator (`vstimecmp` / RV32 `vstimecmph`, `Sstc`).
    ///
    /// The [guest timer](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#vstimecmp)
    /// becomes pending when the unsigned sum of `time` and `htimedelta`,
    /// modulo 2^64, reaches or exceeds the comparator. A change in the
    /// comparison can take time to appear in `hip.VSTIP`.
    ///
    /// # Access requirements
    ///
    /// HS access requires [`mcounteren.TM`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#norm:mcounteren_tm_clr)
    /// and [`menvcfg.STCE`](https://github.com/riscv/riscv-isa-manual/blob/07531fdab1b9e2c3c2306fd3805dfeea1a39b42f/src/priv/machine.adoc#L2223-L2229).
    /// VS accesses the comparator through `stimecmp` and additionally requires
    /// [`hcounteren.TM`](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#norm:hcounteren_acc)
    /// and [`henvcfg.STCE`](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#norm:henvcfg-stce).
    /// Serialize comparator and time-offset updates with the guest's execution.
    Vstimecmp { (0x24d, 0x25d), timer64, r: u64, w: u64 };
}
