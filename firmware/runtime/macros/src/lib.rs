//! Entry-point proc-macro for the RustSBI firmware Runtime.
//!
//! [`entry`] generates the reset entry and the first policy handoff.

mod entry;

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
