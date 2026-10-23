//! Entry-point proc-macro for the RustSBI firmware Runtime.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, MetaNameValue, Token, parse::Parser, punctuated::Punctuated};

/// Marks the firmware policy entry point.
///
/// Every entering hart invokes the policy with its hart ID, device-tree handoff,
/// and loader information address after Runtime installs the local trap entry.
///
/// Supply `initialize`, `stack_size`, and `link_start` arguments.
/// The unsafe initialization hook receives those three entry arguments and a
/// static bootstrap-stack reference. It runs under the bootstrap lock before
/// per-hart stacks exist, and must initialize Runtime stacks before returning
/// a nonzero value for the selected boot hart. A zero return leaves boot pending.
/// The consumer owns loader decoding, heap initialization, and policy records.
///
/// The linker must provide Runtime's firmware image and BSS boundary symbols.
#[proc_macro_attribute]
pub fn entry(
    attribute: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    expand(attribute.into(), item.into()).into()
}

fn expand(attribute: TokenStream, item: TokenStream) -> TokenStream {
    let Ok(options) = Punctuated::<MetaNameValue, Token![,]>::parse_terminated.parse2(attribute)
    else {
        return quote! { compile_error!("expected named entry arguments"); };
    };
    let (mut initialize_hook, mut stack_size, mut link_start) = (None, None, None);
    for option in options {
        let slot = if option.path.is_ident("initialize") {
            &mut initialize_hook
        } else if option.path.is_ident("stack_size") {
            &mut stack_size
        } else if option.path.is_ident("link_start") {
            &mut link_start
        } else {
            return quote! { compile_error!("unknown entry argument"); };
        };
        if slot.replace(option.value).is_some() {
            return quote! { compile_error!("duplicate entry argument"); };
        }
    }
    let (Some(initialize_hook), Some(stack_size), Some(link_start)) =
        (initialize_hook, stack_size, link_start)
    else {
        return quote! { compile_error!("entry requires initialize, stack_size, and link_start"); };
    };
    let Ok(policy) = syn::parse2::<ItemFn>(item) else {
        return quote! { compile_error!("`entry` can be applied only to a function"); };
    };
    let policy_name = &policy.sig.ident;
    let start = include_str!("entry/start.S");
    let relocation = include_str!("entry/relocation.S");

    quote! {
        #policy

        const _: () = {
            #[used]
            #[unsafe(link_section = ".bss.stack")]
            static BOOT_STACK: runtime::boot::BootStack<
                { #stack_size },
            > = runtime::boot::BootStack::uninit();

            static BOOT_LOCK: ::core::sync::atomic::AtomicU32 = ::core::sync::atomic::AtomicU32::new(0);
            static BOOT_READY: ::core::sync::atomic::AtomicU32 = ::core::sync::atomic::AtomicU32::new(0);
            // Called only under BOOT_LOCK, before any hart enters policy.
            extern "C" fn initialize(
                hart_id: usize,
                device_tree: runtime::DeviceTreeHandoff,
                dynamic_info_address: usize,
            ) -> usize {
                // SAFETY: Startup owns the bootstrap stack under BOOT_LOCK,
                // keeps MIE clear, and has not published BOOT_READY.
                unsafe { #initialize_hook(hart_id, device_tree, dynamic_info_address, &BOOT_STACK) }
            }

            /// Invokes this hart's policy after installing Runtime's trap entry.
            #[doc(hidden)]
            #[unsafe(export_name = "__rustsbi_runtime_main")]
            extern "C" fn __rustsbi_runtime_main(
                hart_id: usize,
                device_tree: runtime::DeviceTreeHandoff,
                dynamic_info_address: usize,
            ) {
                // SAFETY: Startup has selected this hart's Runtime stack;
                // machine interrupts are masked and mscratch is zero.
                unsafe { runtime::boot::prepare_traps() };
                let policy_entry: fn(usize, runtime::DeviceTreeHandoff, usize) = #policy_name;
                policy_entry(hart_id, device_tree, dynamic_info_address)
            }

            /// Applies the linker's relative relocations before Rust memory is
            /// initialized.
            ///
            /// # Safety
            ///
            /// Called exactly once by the elected bootstrap hart from `_start`,
            /// before BSS, stacks, or any Rust reference exist.
            #[doc(hidden)]
            #[unsafe(naked)]
            unsafe extern "C" fn relocation_update() {
                ::core::arch::naked_asm!(
                    #relocation,
                    R_RISCV_RELATIVE = const R_RISCV_RELATIVE,
                    START_ADDRESS = const #link_start,
                    XLEN = const usize::BITS,
                )
            }

            const R_RISCV_RELATIVE: usize = 3;

            /// Enters firmware through the architectural boot protocol.
            // The linker script selects this symbol through ENTRY(_start).
            ///
            /// # Safety
            ///
            /// - Pass `a0 = hart ID`, `a1 = DTB pointer`, and
            ///   `a2 = DynamicInfo pointer` (dynamic mode only).
            /// - The selected DTB must describe the hardware and remain readable
            ///   and unchanged through platform initialization.
            /// - Provide exclusively writable DTB storage at the configured
            ///   destination with its declared capacity, or at the boot hart's
            ///   incoming `a1` with a readable FDT and its initial `totalsize + 1024`
            ///   bytes of storage. Keep it valid until the next stage finishes
            ///   using it, outside firmware, stacks, the complete next-stage image,
            ///   and other live objects.
            /// - An M-mode next-stage target must accept architectural entry with
            ///   no Rust stack and only a0/a1 defined, establish its own calling
            ///   context, and preserve all live firmware allocations and references.
            /// - Reserve RAM immediately after the linked image for all discovered
            ///   harts' stacks, without overlapping any other live objects.
            /// - Do not access Rust memory before relocation, BSS initialization,
            ///   and stack setup complete.
            #[doc(hidden)]
            #[unsafe(naked)]
            #[unsafe(link_section = ".text.entry")]
            #[unsafe(export_name = "_start")]
            unsafe extern "C" fn __rustsbi_runtime_start() -> ! {
                ::core::arch::naked_asm!(
                    #start,
                    early_vector = sym runtime::boot::fail_stop,
                    relocation_update = sym relocation_update,
                    locate_stack = sym runtime::boot::locate_stack,
                    boot_stack = sym BOOT_STACK,
                    stack_size = const #stack_size,
                    boot_lock = sym BOOT_LOCK,
                    boot_ready = sym BOOT_READY,
                    initialize = sym initialize,
                    main = sym __rustsbi_runtime_main,
                    finish_boot = sym runtime::boot::finish_boot,
                    XLEN = const usize::BITS,
                )
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{TokenStream, expand};

    const POLICY_FN: &str = "fn main(boot: crate::firmware::BootInfo) { loop {} }";

    #[test]
    fn rejects_invalid_attribute_uses() {
        let arguments = expand("unexpected".parse().unwrap(), POLICY_FN.parse().unwrap());
        assert!(arguments.to_string().contains("compile_error"));
        let expanded = expand(TokenStream::new(), "struct S;".parse().unwrap());
        assert!(expanded.to_string().contains("compile_error"));
    }
}
