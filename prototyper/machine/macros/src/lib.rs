//! Compile-time support for the unique Prototyper machine entry.

use proc_macro::{TokenStream, TokenTree};

/// Defines the unique safe firmware-policy entry.
///
/// The generated raw symbol is a fixed trampoline into the machine crate. The
/// generated Rust bridge enforces the exact `fn(machine::BootInfo) -> !` type
/// when the invoking firmware is compiled.
#[proc_macro_attribute]
pub fn entry(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return compile_error("`entry` does not accept arguments");
    }
    let entry = match function_name(&item) {
        Ok(function) => function,
        Err(message) => return compile_error(&message),
    };

    entry_expansion(&entry, &item.to_string())
        .parse()
        .expect("the fixed entry expansion must remain valid Rust")
}

fn function_name(item: &TokenStream) -> Result<String, String> {
    let mut tokens = item.clone().into_iter();
    loop {
        let Some(token) = tokens.next() else {
            return Err("`entry` can be applied only to a function".into());
        };
        if matches!(&token, TokenTree::Ident(ident) if ident.to_string() == "fn") {
            let Some(TokenTree::Ident(function)) = tokens.next() else {
                return Err("expected a function name after `fn` in `entry`".into());
            };
            return Ok(function.to_string());
        }
    }
}

fn entry_expansion(entry: &str, item: &str) -> String {
    format!(
        r#"
{item}

const _: () = {{
    #[doc(hidden)]
    #[unsafe(export_name = "__rustsbi_prototyper_main")]
    extern "Rust" fn __rustsbi_prototyper_main(
        boot: ::machine::BootInfo,
    ) -> ! {{
        let entry: fn(::machine::BootInfo) -> ! = {entry};
        entry(boot)
    }}

    #[doc(hidden)]
    ///
    /// # Safety
    ///
    /// The previous machine stage must enter with the documented RustSBI raw
    /// register envelope and stable boot inputs. This symbol may be entered
    /// only as the image's architectural `_start`; the machine trampoline
    /// establishes relocation, stacks, BSS, and Rust invariants before it
    /// calls safe firmware policy.
    #[unsafe(naked)]
    #[unsafe(link_section = ".text.entry")]
    #[unsafe(export_name = "_start")]
    unsafe extern "C" fn __rustsbi_prototyper_start() -> ! {{
        unsafe extern "C" {{
            fn __rustsbi_prototyper_from_previous() -> !;
        }}
        // SAFETY: this naked function emits only a direct branch. It preserves
        // every entry register and cannot execute a compiler-generated
        // prologue before the machine TCB establishes a Rust stack.
        ::core::arch::naked_asm!(
            "j {{entry}}",
            entry = sym __rustsbi_prototyper_from_previous,
        )
    }}
}};
"#
    )
}

fn compile_error(message: &str) -> TokenStream {
    format!("compile_error!({message:?});")
        .parse()
        .expect("the fixed diagnostic must remain valid Rust")
}

#[cfg(test)]
mod tests {
    use super::entry_expansion;

    #[test]
    fn entry_expansion_keeps_the_policy_function_and_adds_one_raw_and_one_typed_entry() {
        let expanded = entry_expansion("main", "fn main(boot: machine::BootInfo) -> ! { loop {} }");
        assert!(expanded.contains("fn main(boot: machine::BootInfo) -> ! { loop {} }"));
        assert_eq!(expanded.matches("export_name = \"_start\"").count(), 1);
        assert_eq!(
            expanded
                .matches("export_name = \"__rustsbi_prototyper_main\"")
                .count(),
            1
        );
        assert!(expanded.contains("let entry: fn(::machine::BootInfo) -> ! = main;"));
        assert!(expanded.contains("sym __rustsbi_prototyper_from_previous"));
    }
}
