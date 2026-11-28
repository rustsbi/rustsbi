//! Endianness and privilege encodings shared by status registers.
//!
//! Shared fields of M/S/VS status registers, where present:
//!
//! | Field | Meaning |
//! | --- | --- |
//! | `SIE` | Supervisor interrupt enable. |
//! | `SPIE` | Previous Supervisor interrupt enable. |
//! | `UBE` | User-mode memory byte order. |
//! | `SPP` | Privilege before a trap into Supervisor mode. |
//! | `SUM` | Permit Supervisor loads and stores to user pages. Supervisor instruction fetches from user pages remain forbidden. |
//! | `MXR` | Let loads use execute permission as well as read permission in page tables. |
//!
//! In virtual-supervisor status, Supervisor and User refer to VS and VU modes.

field_enum! {
    /// Privilege level encoded in `mstatus.MPP` and firmware entry state.
    pub enum Privilege {
        /// Unprivileged application execution.
        User = 0,
        /// Operating-system or hypervisor execution.
        Supervisor = 1,
        /// Firmware execution at the highest privilege level.
        Machine = 3,
    }
}

field_enum! {
    rw
    /// Memory byte order in the `UBE`, `SBE`, and `VSBE` status fields.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-6-machine-status-mstatus-and-mstatush-registers>.
    pub enum Endianness {
        /// Places the least significant byte at the lowest address.
        Little = 0,
        /// Places the most significant byte at the lowest address.
        Big = 1,
    }
}

field_enum! {
    rw
    /// Previous privilege encoded in one bit (`sstatus.SPP`, `mstatus.SPP`, `hstatus.SPVP`, `vsstatus.SPP`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#sstatus>.
    pub enum SupervisorPrivilege {
        /// Includes Virtual User mode.
        User = 0,
        /// Includes Virtual Supervisor mode.
        Supervisor = 1,
    }
}
