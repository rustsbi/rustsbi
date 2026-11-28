//! Machine indirect CSR selection and data aliases.

define_csr! {
    /// Machine indirect CSR selector (`miselect`, `Smcsrind`/`Smaia`).
    ///
    /// # Access requirements
    ///
    /// Serialize `miselect` updates with subsequent `mireg*` accesses on this hart
    /// (typically under disabled machine interrupts).
    Miselect { 0x350, r: usize, w: usize };

    Mireg { 0x351 };

    Mireg2 { 0x352 };

    Mireg3 { 0x353 };

    Mireg4 { 0x355 };

    Mireg5 { 0x356 };

    Mireg6 { 0x357 };
}
