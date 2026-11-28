//! Cache-block operation controls shared by environment configuration registers.
//!
//! The privilege level and controlling register determine the exception when
//! execution is prohibited; virtual instruction exceptions are possible in guests.
//!
//! Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#sec:menvcfg>.

field_enum! {
    /// Cache-block invalidate mode encoded in `envcfg.CBIE` (`Zicbom`).
    pub enum CacheBlockInvalidate {
        /// Prohibits `cbo.inval` at the controlled privilege level.
        Disabled = 0,
        /// Requires an otherwise permitted `cbo.inval` to flush.
        Flush = 1,
        /// Permits invalidation, subject to other applicable controls.
        Invalidate = 3,
    }
}
