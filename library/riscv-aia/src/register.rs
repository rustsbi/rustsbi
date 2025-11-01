//! RISC-V AIA CSRs.

// -- `Smaia` CSRs --

// Machine-level window to indirectly accessed registers
// TODO pub mod miselect;
// TODO pub mod mireg;

// Machine-level interrupts
// TODO pub mod mie;
// TODO pub mod mip;
pub mod mtopei;
pub mod mtopi;

// Machine-level high-half CSRs, RV32 only.
pub mod midelegh;
// TODO pub mod mieh;
// TODO pub mod miph;
// TODO pub mod mvieh;
// TODO pub mod mviph;

// -- `Ssaia` CSRs --

// Supervisor-level window to indirectly accessed registers
// TODO pub mod siselect;
// TODO pub mod sireg;

// Supervisor-level interrupts
// TODO pub mod sie;
// TODO pub mod sip;
// TODO pub mod stopei;
// TODO pub mod stopi;

// Supervisor-level high-half CSRs, RV32 only.
// TODO pub mod sieh;
// TODO pub mod siph;

// -- Hypervisor and VS CSRs --

// Delegated and virtual interrupts, interrupt priorities, for VS-level
// TODO pub mod hideleg;
// TODO pub mod hvien;
// TODO pub mod hvictl;
// TODO pub mod hvip;
// TODO pub mod hviprio1;
// TODO pub mod hviprio2;

// VS-level window to indirectly accessed registers
// TODO pub mod vsiselect;
// TODO pub mod vsireg;

// VS-level interrupts
// TODO pub mod vsie;
// TODO pub mod vsip;
// TODO pub mod vstopei;
// TODO pub mod vstopi;

// Hypervisor and VS-level high-half CSRs, RV32 only.
// TODO pub mod hidelegh;
// TODO pub mod hvienh;
// TODO pub mod hviph;
// TODO pub mod hviprio1h;
// TODO pub mod hviprio2h;
// TODO pub mod vsieh;
// TODO pub mod vsiph;
