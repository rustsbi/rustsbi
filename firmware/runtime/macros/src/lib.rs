//! Proc macros for the RustSBI firmware Runtime.
//!
//! [`entry`] generates the reset entry and the first policy handoff.
//! [`sbi_policy`] publishes the SBI dispatcher and exports the linked ecall
//! handler.

mod entry;
mod sbi_policy;

/// Marks the firmware policy entry point.
///
/// The policy receives the boot hart's validated device-tree handoff.
#[proc_macro_attribute]
pub fn entry(
    attribute: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    crate::entry::expand(attribute.into(), item.into()).into()
}

/// Apply this attribute to a non-generic structure implementing
/// `runtime::rustsbi::RustSBI + Sync + 'static`. The generated `publish`
/// method publishes one static instance before any hart handles an SBI call.
/// Exactly one policy must be linked into the firmware.
#[proc_macro_attribute]
pub fn sbi_policy(
    attribute: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    crate::sbi_policy::expand(attribute.into(), item.into()).into()
}
