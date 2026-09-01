# `spacemit-riscv`

Low-level access to custom RISC-V CSRs used by SpacemiT K1 and K3 application cores in RV64 firmware.

CSR addresses and names are taken from public firmware; field meanings and access semantics are inferred from observed use because no complete public hardware specification is available. Support may vary by core and chip revision.

Callers must confirm that the current hart implements a CSR; the `try_*` functions cannot catch illegal-instruction traps caused by an unimplemented CSR.

[![crates.io](https://img.shields.io/crates/v/spacemit-riscv.svg)](https://crates.io/crates/spacemit-riscv)
[![Documentation](https://docs.rs/spacemit-riscv/badge.svg)](https://docs.rs/spacemit-riscv)
![License](https://img.shields.io/crates/l/spacemit-riscv.svg)

## License

This project is licensed under either of

- MIT license ([LICENSE-MIT](https://github.com/rustsbi/rustsbi/blob/main/LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
- Mulan PSL v2 ([LICENSE-MULAN](https://github.com/rustsbi/rustsbi/blob/main/LICENSE-MULAN) or [https://opensource.org/licenses/MulanPSL-2.0](https://opensource.org/licenses/MulanPSL-2.0))
