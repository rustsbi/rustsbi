//! Virtual-supervisor indirect CSR selection and data aliases.

define_csr! {
    /// Virtual supervisor indirect CSR selector (`vsiselect`, `Sscsrind`/`AIA`).
    ///
    /// # Access requirements
    ///
    /// Serialize selection with subsequent `vsireg*` accesses.
    Vsiselect { 0x250, r: usize, w: usize };

    Vsireg { 0x251 };

    Vsireg2 { 0x252 };

    Vsireg3 { 0x253 };

    Vsireg4 { 0x255 };

    Vsireg5 { 0x256 };

    Vsireg6 { 0x257 };
}
