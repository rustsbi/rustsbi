//! RISC-V instruction-set architecture specifications.
//!
//! Shared environment-configuration fields, where present:
//!
//! | Field | Meaning |
//! | --- | --- |
//! | `FIOM` | For the affected privilege modes, ordering device input or output also orders memory reads or writes, respectively. |
//! | `LPE` | Landing-pad enable (`Zicfilp`). |
//! | `SSE` | Shadow-stack enable (`Zicfiss`). |
//! | `CBIE` | Cache-block invalidate mode (`Zicbom`); see [`values::cache_block`]. |
//! | `CBCFE` | Cache-block clean/flush enable (`Zicbom`). |
//! | `CBZE` | Cache-block zero enable (`Zicboz`). |

pub mod privileged;
pub mod unprivileged;
pub mod values;
