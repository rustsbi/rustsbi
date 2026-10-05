//! Entry-point proc-macro for the RustSBI Prototyper firmware.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

/// The firmware policy entry point.
///
/// Generated startup code validates the boot hart's device-tree pointer before
/// policy code can inspect it.
#[proc_macro_attribute]
pub fn entry(
    attribute: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    expand(attribute.into(), item.into()).into()
}

fn expand(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return quote! { compile_error!("`entry` does not accept arguments"); };
    }
    let Ok(policy) = syn::parse2::<ItemFn>(item) else {
        return quote! { compile_error!("`entry` can be applied only to a function"); };
    };
    let policy_name = &policy.sig.ident;

    quote! {
        #policy

        const _: () = {
            /// Connects Firmware Entry to the policy function.
            #[doc(hidden)]
            #[unsafe(export_name = "__rustsbi_prototyper_main")]
            extern "C" fn __rustsbi_prototyper_main(
                _hart_id: usize,
                device_tree: runtime::DeviceTreeHandoff,
                dynamic_info_address: usize,
            ) {
                // Boot register handoff from the previous stage:
                //   a0 = hart ID, a1 = device tree pointer (base RISC-V
                //   convention); a2 = DynamicInfo address (fw_dynamic
                //   extension, dynamic variant only).
                //
                // `_hart_id` keeps the first ABI argument position but its value is unused:
                // arguments bind by position, so removing the parameter
                // would shift `a1`/`a2`. In M-mode the hart ID is always
                // re-read from the mhartid CSR, and it is re-materialized
                // into `a0` when this firmware boots the next stage (see
                // `sbi::trap::boot`) — S-mode cannot read mhartid and
                // depends on receiving its hart ID this way.
                let policy_entry: fn(crate::firmware::BootInfo) = #policy_name;
                let boot = match crate::firmware::BootInfo::decode(
                    device_tree,
                    dynamic_info_address,
                ) {
                    Ok(boot) => boot,
                    Err(error) => {
                        panic!("firmware entry rejected the platform description: {error}")
                    }
                };
                policy_entry(boot)
            }

            /// Applies the linker's relative relocations before Rust memory
            /// is initialized.
            ///
            /// # Safety
            ///
            /// Called exactly once by the elected boot hart from `_start`,
            /// before BSS, stacks, or any Rust reference exist.
            #[doc(hidden)]
            #[unsafe(naked)]
            unsafe extern "C" fn relocation_update() {
                ::core::arch::naked_asm!(
                    include_str!("entry/relocation.S"),
                    R_RISCV_RELATIVE = const R_RISCV_RELATIVE,
                    START_ADDRESS = const crate::cfg::SBI_LINK_START_ADDRESS,
                    XLEN = const usize::BITS,
                )
            }

            const R_RISCV_RELATIVE: usize = 3;

            /// Reads a preferred relocation hart from the dynamic handoff.
            /// This code runs before Rust memory or a stack is available.
            #[cfg(not(any(feature = "payload", feature = "jump")))]
            #[doc(hidden)]
            #[unsafe(naked)]
            unsafe extern "C" fn preferred_relocation_hart(
                _hart_id: usize,
                _device_tree_address: usize,
                _dynamic_info_address: usize,
            ) -> usize {
                ::core::arch::naked_asm!(
                    include_str!("entry/dynamic_boot_hart.S"),
                    DYNAMIC_INFO_MAGIC = const crate::firmware::dynamic::DYNAMIC_INFO_MAGIC,
                    DYNAMIC_INFO_VERSION_2 = const crate::firmware::dynamic::DYNAMIC_INFO_VERSION_2,
                    DYNAMIC_INFO_VERSION_OFFSET = const crate::firmware::dynamic::DYNAMIC_INFO_VERSION_OFFSET,
                    DYNAMIC_INFO_BOOT_HART_OFFSET = const crate::firmware::dynamic::DYNAMIC_INFO_BOOT_HART_OFFSET,
                    XLEN = const usize::BITS,
                )
            }

            /// Payload and jump firmware have no dynamic relocation preference.
            #[cfg(any(feature = "payload", feature = "jump"))]
            #[doc(hidden)]
            #[unsafe(naked)]
            unsafe extern "C" fn preferred_relocation_hart(
                _hart_id: usize,
                _device_tree_address: usize,
                _dynamic_info_address: usize,
            ) -> usize {
                ::core::arch::naked_asm!("li a0, -1", "ret")
            }

            /// Architectural firmware entry point, referenced by
            /// `ENTRY(_start)` in the linker script generated by `build.rs`.
            ///
            /// # Safety
            ///
            /// The previous stage must enter with the register envelope of
            /// the selected boot protocol: `a0 = hart ID`, `a1 = device
            /// tree pointer` (base RISC-V convention), `a2 = DynamicInfo
            /// address` (fw_dynamic extension, dynamic variant only).
            /// The boot hart's selected device tree must remain writable and
            /// exclusively owned through platform initialization, and its
            /// enabled memory and device descriptions must match the hardware.
            /// Runs before relocation, BSS, and stacks exist, so the
            /// assembly may not touch Rust memory until those steps
            /// complete.
            #[doc(hidden)]
            #[unsafe(naked)]
            #[unsafe(link_section = ".text.entry")]
            #[unsafe(export_name = "_start")]
            unsafe extern "C" fn __rustsbi_prototyper_start() -> ! {
                ::core::arch::naked_asm!(
                    include_str!("entry/start.S"),
                    early_vector = sym runtime::boot::fail_stop,
                    preferred_relocation_hart = sym preferred_relocation_hart,
                    relocation_update = sym relocation_update,
                    locate_stack = sym runtime::boot::locate_stack,
                    main = sym __rustsbi_prototyper_main,
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
