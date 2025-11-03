//! Machine indirect register select (miselect)

riscv::read_write_csr! {
    /// Machine indirect register select.
    Miselect: 0x350,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

// Note: miselect controls which register is accessed via `mireg`.
