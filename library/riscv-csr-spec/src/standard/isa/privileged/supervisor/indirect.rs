//! Supervisor indirect CSR selection and data aliases.

define_csr! {
    /// Supervisor indirect CSR selector (`siselect`, `Sscsrind`/`Ssaia`).
    ///
    /// # Access requirements
    ///
    /// Serialize selection with subsequent `sireg*` accesses on this hart.
    Siselect { 0x150, r: usize, w: usize };

    Sireg { 0x151 };

    Sireg2 { 0x152 };

    Sireg3 { 0x153 };

    Sireg4 { 0x155 };

    Sireg5 { 0x156 };

    Sireg6 { 0x157 };
}
