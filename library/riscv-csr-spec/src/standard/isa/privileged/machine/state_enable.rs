//! Lower-privilege state-access controls.

define_csr! {
    Mstateen0 { (0x30c, 0x31c), r, w } {
        /// Custom state enable.
        C[0] { rw: bool }
        /// Access to `fcsr` and floating-point instructions under `Zfinx`.
        FCSR[1] { rw: bool }
        /// Jump vector table state enable (`Zcmt`).
        JVT[2] { rw: bool }
        /// Lower-privilege access to [`hedelegh`](https://docs.riscv.org/reference/isa/v20260120/priv/smstateen.html#norm:mstateen0-p1p13_op) on RV32.
        P1P13[56] { rw: bool }
        /// Access to the debug context registers `scontext` and `hcontext`.
        CONTEXT[57] { rw: bool }
        /// Lower-privilege access to IMSIC state, including `stopei` and `vstopei`.
        IMSIC[58] { rw: bool }
        /// Lower-privilege access to Ssaia state outside the IMSIC and indirect CSR controls.
        AIA[59] { rw: bool }
        /// Access to the supervisor and virtual-supervisor indirect CSR windows.
        CSRIND[60] { rw: bool }
        /// Access to `senvcfg`, `henvcfg`, and the RV32 high word `henvcfgh`.
        ENVCFG[62] { rw: bool }
        SE0[63] { rw: bool }
    }

    Mstateen1 { (0x30d, 0x31d), r, w } {
        SE0[63] { rw: bool }
    }

    Mstateen2 { (0x30e, 0x31e), r, w } {
        SE0[63] { rw: bool }
    }

    Mstateen3 { (0x30f, 0x31f), r, w } {
        SE0[63] { rw: bool }
    }
}
