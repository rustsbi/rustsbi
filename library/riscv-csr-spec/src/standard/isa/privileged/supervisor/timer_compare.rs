//! Supervisor timer comparator.

define_csr! {
    /// Supervisor timer comparator (`stimecmp` / RV32 `stimecmph`, `Sstc`).
    ///
    /// STIP reflects the unsigned comparison `time >= stimecmp`, possibly
    /// after a delay. The RV32 write sequence places all ones in the low
    /// word before changing the high word so no intermediate deadline is
    /// earlier than both the old and new deadlines.
    ///
    /// # Access requirements
    ///
    /// S/HS-mode access to [the comparator](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#stimecmp)
    /// requires Sstc, [`menvcfg.STCE`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#norm:menvcfg_stce_op2)
    /// and [`mcounteren.TM`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#norm:mcounteren_tm_clr).
    /// Serialize updates to this hart's deadline.
    Stimecmp { (0x14d, 0x15d), timer64, r: u64, w: u64 };
}
