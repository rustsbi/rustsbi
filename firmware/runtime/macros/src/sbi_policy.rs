//! `#[sbi_policy]` expansion.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ItemStruct;

pub(crate) fn expand(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return quote! { compile_error!("`sbi_policy` does not accept arguments"); };
    }
    let Ok(policy) = syn::parse2::<ItemStruct>(item.clone()) else {
        return quote! { compile_error!("`sbi_policy` requires a non-generic structure"); };
    };
    if !policy.generics.params.is_empty() || policy.generics.where_clause.is_some() {
        return quote! { compile_error!("`sbi_policy` structure must not be generic"); };
    }
    let policy_name = policy.ident;
    // Do not let a generated identifier shadow the annotated item.
    let name = policy_name.to_string();
    let name = name.trim_start_matches("r#");
    let ident = |base: &str| {
        if name == base {
            format_ident!("{base}_")
        } else {
            format_ident!("{base}")
        }
    };
    let entry = ident("__rustsbi_policy_entry");
    let extension = ident("__rustsbi_policy_extension");
    let function = ident("__rustsbi_policy_function");
    let args = ident("__rustsbi_policy_args");
    quote! {
        #item

        const _: () = {
            static POLICY: ::core::sync::atomic::AtomicPtr<#policy_name> =
                ::core::sync::atomic::AtomicPtr::new(::core::ptr::null_mut());

            impl #policy_name {
                /// Publishes this policy once for SBI calls on every hart.
                pub(crate) fn publish(&'static self) -> runtime::Result<()> {
                    fn assert_sync<T: Sync + 'static>() {}
                    assert_sync::<#policy_name>();
                    POLICY.compare_exchange(
                        ::core::ptr::null_mut(),
                        ::core::ptr::from_ref(self).cast_mut(),
                        ::core::sync::atomic::Ordering::AcqRel,
                        ::core::sync::atomic::Ordering::Acquire,
                    ).map(|_| ()).map_err(|_| runtime::Error::AlreadyInitialized)
                }

                /// Returns the published policy after platform initialization.
                pub(crate) fn get() -> Option<&'static Self> {
                    let policy = POLICY.load(::core::sync::atomic::Ordering::Acquire);
                    // SAFETY: Publication stores only a static reference of this type.
                    unsafe { policy.as_ref() }
                }
            }

            #[unsafe(export_name = "__rustsbi_runtime_handle_ecall")]
            unsafe extern "Rust" fn #entry(
                #extension: usize,
                #function: usize,
                #args: [usize; 6],
            ) -> runtime::rustsbi::SbiRet {
                let policy = POLICY.load(::core::sync::atomic::Ordering::Acquire);
                if policy.is_null() {
                    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                    // SAFETY: The fail-stop entry is an assembly jump target
                    // and does not access a policy or return.
                    unsafe {
                        ::core::arch::asm!(
                            "j {fail}",
                            fail = sym runtime::boot::fail_stop,
                            options(noreturn),
                        )
                    }
                    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                    panic!("SBI policy not published");
                }
                // SAFETY: Only `publish` writes this pointer, from a static
                // reference of this exact Sync type. Acquire observes publication.
                let policy = unsafe { &*policy };
                runtime::rustsbi::RustSBI::handle_ecall(policy, #extension, #function, #args)
            }
        };
    }
}
