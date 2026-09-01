//! Physical memory attribute configuration 2 register.
//!
//! # Platform support
//!
//! Public K3 firmware declares this register for SpacemiT X100 and A100 cores.
//! No direct platform access or per-core field layout has been found.
//!
//! The name and address are taken from public K3 firmware. Its field layout is
//! unknown, and associating it with particular PMA entries would be an
//! unverified inference, so this module exposes only raw register access.
//!
//! [K3 OpenSBI declaration](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/platform/generic/include/spacemit/k3/core_common.h#L44-L45)

riscv::read_write_csr_as_usize!(0x7df);
