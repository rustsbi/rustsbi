//! Unprivileged counters and entropy-source CSR definitions.
//!
//! The [Zicntr and Zihpm counters](https://docs.riscv.org/reference/isa/v20260120/unpriv/counters.html#counters)
//! expose read-only values. Polling the [Zkr entropy source](https://docs.riscv.org/reference/isa/v20260120/unpriv/scalar-crypto.html#crypto_scalar_seed_csr)
//! requires a CSR read-write operation.

mod counter_timer;
mod entropy;

pub use counter_timer::*;
pub use entropy::*;
