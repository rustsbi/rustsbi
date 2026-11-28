//! `#[entry]` expansion.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

pub(crate) fn expand(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return quote! { compile_error!("`entry` does not accept arguments"); };
    }
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
                { crate::cfg::STACK_SIZE_PER_HART },
            > = runtime::boot::BootStack::uninit();

            static BOOT_LOCK: ::core::sync::atomic::AtomicU32 = ::core::sync::atomic::AtomicU32::new(0);
            static BOOT_READY: ::core::sync::atomic::AtomicU32 = ::core::sync::atomic::AtomicU32::new(0);
            static BOOT_INFO: spin::Mutex<Option<crate::firmware::BootInfo>> = spin::Mutex::new(None);
            static BOOT_HART: spin::Once<usize> = spin::Once::new();

            // Called only under BOOT_LOCK, before any hart enters policy.
            extern "C" fn initialize(
                hart_id: usize,
                device_tree: runtime::DeviceTreeHandoff,
                dynamic_info_address: usize,
            ) -> usize {
                // SAFETY: In dynamic mode the entry contract keeps this hart's
                // loader record readable and authorizes its next-stage target.
                let mut boot = unsafe { crate::firmware::BootInfo::from_entry(dynamic_info_address) };
                // Inspecting an FDT with NOP tokens can allocate during selection.
                crate::heap::init();
                if !boot.select_boot_hart(hart_id, device_tree)
                    .expect("firmware entry rejected the platform description") {
                    return 0;
                }
                let (_, next_stage_entry) = boot.next_stage();
                let handoff = boot.dynamic_info_range();
                // SAFETY: The loader's entry contract grants exclusive RAM
                // for firmware and stacks. BOOT_LOCK serializes candidates;
                // BOOT_READY remains clear until this Rust frame is retired.
                unsafe {
                    runtime::boot::initialize_stacks(
                        &BOOT_STACK,
                        crate::cfg::HART_CAPACITY,
                        boot.platform_description_mut(),
                        next_stage_entry,
                        handoff,
                    )
                }.expect("firmware could not initialize its hart topology");
                BOOT_HART.call_once(|| hart_id);
                *BOOT_INFO.lock() = Some(boot);
                1
            }

            /// Invokes firmware policy with the decoded boot handoff.
            #[doc(hidden)]
            #[unsafe(export_name = "__rustsbi_prototyper_main")]
            extern "C" fn __rustsbi_prototyper_main(
                hart_id: usize,
                _device_tree: runtime::DeviceTreeHandoff,
                dynamic_info_address: usize,
            ) {
                // SAFETY: Startup has selected this hart's Runtime stack;
                // machine interrupts are masked and mscratch is zero.
                unsafe { runtime::boot::prepare_traps() };
                let policy_entry: fn(crate::firmware::BootInfo) = #policy_name;
                let boot = if BOOT_HART.get().copied() == Some(hart_id) {
                    BOOT_INFO.lock().take().expect("boot handoff consumed once")
                } else {
                    // SAFETY: The same loader contract covers a secondary hart's
                    // later copy of its own dynamic record.
                    unsafe { crate::firmware::BootInfo::from_entry(dynamic_info_address) }
                };
                policy_entry(boot)
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
                    START_ADDRESS = const crate::cfg::SBI_LINK_START_ADDRESS,
                    XLEN = const usize::BITS,
                )
            }

            const R_RISCV_RELATIVE: usize = 3;

            /// Enters firmware through the architectural boot protocol.
            ///
            /// # Safety
            ///
            /// - Enter in M-mode with `mstatus.MIE` and `MPRV` clear.
            ///   All entering harts must see coherent firmware memory and support
            ///   the word atomics and `FENCE.I` used during startup.
            ///   Give startup exclusive writable storage for its synchronization
            ///   words in `.text.entry`, relocation targets, BSS and the secondary
            ///   hart stacks immediately after the linked image.
            /// - Pass the DTB address in `a1`. Startup reads the hart ID from `mhartid`.
            ///   In dynamic mode, `a2` must name a word-aligned, initialized v1
            ///   (five-word) or v2 (six-word) record, readable and unchanged until
            ///   that hart finishes copying it. Authorize its target in the specified
            ///   privilege mode. An M-mode payload must preserve live firmware memory.
            /// - Every DTB used for discovery must describe the actual hardware
            ///   and remain readable and unchanged until handoff consumes it.
            ///   Give the selected board policy control of its PMP entries
            ///   and any cache, memory-attribute and interrupt controls it programs.
            /// - Provide exclusive writable DTB storage at the configured destination.
            ///   Without one, each candidate that inspects its incoming `a1` needs
            ///   a readable FDT there with writable capacity of `totalsize + 1024` bytes.
            ///   Keep the selected destination live through next-stage use and
            ///   disjoint from firmware, all hart stacks, the next-stage image
            ///   and other live objects except the source FDT.
            #[doc(hidden)]
            #[unsafe(naked)]
            #[unsafe(link_section = ".text.entry")]
            #[unsafe(export_name = "_start")]
            unsafe extern "C" fn __rustsbi_prototyper_start() -> ! {
                ::core::arch::naked_asm!(
                    #start,
                    early_vector = sym runtime::boot::fail_stop,
                    relocation_update = sym relocation_update,
                    locate_stack = sym runtime::boot::locate_stack,
                    boot_stack = sym BOOT_STACK,
                    stack_size = const crate::cfg::STACK_SIZE_PER_HART,
                    boot_lock = sym BOOT_LOCK,
                    boot_ready = sym BOOT_READY,
                    initialize = sym initialize,
                    main = sym __rustsbi_prototyper_main,
                    finish_boot = sym runtime::boot::finish_boot,
                    XLEN = const usize::BITS,
                )
            }
        };
    }
}
