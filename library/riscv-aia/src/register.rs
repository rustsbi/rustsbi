//! RISC-V AIA CSRs.

// -- `Smaia` CSRs --

// Machine-level window to indirectly accessed registers
pub mod miselect;
pub mod mireg;

// Machine-level interrupts
pub mod mie;
pub mod mip;
pub mod mtopei;
pub mod mtopi;

// Machine-level high-half CSRs, RV32 only.
pub mod midelegh;
pub mod mieh;
pub mod miph;
pub mod mvieh;
pub mod mviph;

// -- `Ssaia` CSRs --

// Supervisor-level window to indirectly accessed registers
pub mod siselect;
pub mod sireg;

// Supervisor-level interrupts
pub mod sie;
pub mod sip;
pub mod stopei;
pub mod stopi;

// Supervisor-level high-half CSRs, RV32 only.
pub mod sieh;
pub mod siph;

// -- Hypervisor and VS CSRs --

// Delegated and virtual interrupts, interrupt priorities, for VS-level
pub mod hideleg;
pub mod hvien;
pub mod hvictl;
pub mod hvip;
pub mod hviprio1;
pub mod hviprio2;

// VS-level window to indirectly accessed registers
pub mod vsiselect;
pub mod vsireg;

// VS-level interrupts
pub mod vsie;
pub mod vsip;
pub mod vstopei;
pub mod vstopi;

// Hypervisor and VS-level high-half CSRs, RV32 only.
pub mod hidelegh;
pub mod hvienh;
pub mod hviph;
pub mod hviprio1h;
pub mod hviprio2h;
pub mod vsieh;
pub mod vsiph;
