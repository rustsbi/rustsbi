//! Supervisor state-access controls.

define_csr! {
    Sstateen0 { 0x10c, r: usize, w: usize };

    Sstateen1 { 0x10d, r: usize, w: usize };

    Sstateen2 { 0x10e, r: usize, w: usize };

    Sstateen3 { 0x10f, r: usize, w: usize };
}
