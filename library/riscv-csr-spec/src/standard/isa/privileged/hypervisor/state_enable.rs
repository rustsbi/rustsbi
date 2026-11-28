//! Guest state-access controls.

define_csr! {
    Hstateen0 { (0x60c, 0x61c), r, w } {
        /// Custom state enable.
        C[0] { rw: bool }
        /// Guest access to `fcsr` and floating-point instructions under `Zfinx`.
        FCSR[1] { rw: bool }
        /// Jump vector table enable.
        JVT[2] { rw: bool }
        /// VS-mode `scontext` enable.
        CONTEXT[57] { rw: bool }
        /// Guest interrupt-file access, including `stopei` in VS-mode.
        IMSIC[58] { rw: bool }
        /// Guest access to Ssaia state outside the IMSIC and indirect CSR controls.
        AIA[59] { rw: bool }
        /// VS-mode access to the guest indirect CSR window through `siselect` and `sireg*`.
        CSRIND[60] { rw: bool }
        /// VS-mode `senvcfg` access.
        ENVCFG[62] { rw: bool }
        SE0[63] { rw: bool }
    }

    Hstateen1 { (0x60d, 0x61d), r, w } {
        SE0[63] { rw: bool }
    }

    Hstateen2 { (0x60e, 0x61e), r, w } {
        SE0[63] { rw: bool }
    }

    Hstateen3 { (0x60f, 0x61f), r, w } {
        SE0[63] { rw: bool }
    }
}
