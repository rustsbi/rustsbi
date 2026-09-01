//! K3 physical memory attribute address registers.
//!
//! Public K3 firmware declares sixteen consecutive registers. Only
//! `pmaaddr8` has an observed write, in which a physical upper limit is shifted
//! right by two. No matching-mode or permission fields are publicly known.

macro_rules! reg {
    ($addr:expr, $csr:ident $(, $extra:literal)?) => {
        #[doc = concat!("SpacemiT K3 `", stringify!($csr), "` register.")]
        #[doc = ""]
        #[doc = "The address and name are taken from public K3 firmware; no bit-field semantics are exposed."]
        $(#[doc = $extra])?
        #[doc = ""]
        #[doc = "# Platform support"]
        #[doc = ""]
        #[doc = "Public K3 firmware declares this register for SpacemiT X100 and A100 cores. That declaration alone does not confirm implementation on either core family."]
        #[doc = ""]
        #[doc = "[K3 OpenSBI declaration](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/platform/generic/include/spacemit/k3/core_common.h#L47-L62)"]
        pub mod $csr {
            riscv::read_write_csr_as_usize!($addr);
        }
    };
}

reg!(0x7e0, pmaaddr0);
reg!(0x7e1, pmaaddr1);
reg!(0x7e2, pmaaddr2);
reg!(0x7e3, pmaaddr3);
reg!(0x7e4, pmaaddr4);
reg!(0x7e5, pmaaddr5);
reg!(0x7e6, pmaaddr6);
reg!(0x7e7, pmaaddr7);
reg!(
    0x7e8,
    pmaaddr8,
    "K3 U-Boot SPL writes a physical upper limit shifted right by two on the X100 boot path; that encoding is inferred from this single observed use. [K3 U-Boot source](https://github.com/spacemit-com/uboot-2022.10/blob/676971d3a61f2583dfd53d8f7dfbc9607cea74b7/board/spacemit/k3/spl.c#L106-L121)"
);
reg!(0x7e9, pmaaddr9);
reg!(0x7ea, pmaaddr10);
reg!(0x7eb, pmaaddr11);
reg!(0x7ec, pmaaddr12);
reg!(0x7ed, pmaaddr13);
reg!(0x7ee, pmaaddr14);
reg!(0x7ef, pmaaddr15);
