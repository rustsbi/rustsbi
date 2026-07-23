//! Compile-time support for the Prototyper machine entry and test contracts.

use proc_macro::{TokenStream, TokenTree};

/// Declares one privileged test in the dedicated link-time registry.
///
/// The test must have the shape `fn()`. Its stable name is generated from
/// `module_path!()` and the function name. A dedicated firmware invocation
/// selects and runs exactly one such test.
#[proc_macro_attribute]
pub fn mtest(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return compile_error("`mtest` does not accept arguments");
    }
    let function = match function_name(&item) {
        Ok(function) => function,
        Err(message) => return compile_error(message),
    };
    mtest_expansion(&function, &item.to_string())
        .parse()
        .expect("the machine-test expansion must remain valid Rust")
}

fn function_name(item: &TokenStream) -> Result<String, &'static str> {
    let mut tokens = item.clone().into_iter();
    loop {
        let Some(token) = tokens.next() else {
            return Err("`mtest` can be applied only to a function");
        };
        if matches!(&token, TokenTree::Ident(ident) if ident.to_string() == "fn") {
            let Some(TokenTree::Ident(function)) = tokens.next() else {
                return Err("expected a function name after `fn`");
            };
            return Ok(function.to_string());
        }
    }
}

fn mtest_expansion(function: &str, item: &str) -> String {
    let test_wrapper = format!("__rustsbi_mtest_call_{function}");
    let descriptor = format!("__RUSTSBI_MTEST_{function}");
    format!(
        r#"
#[cfg(feature = "mtest")]
{item}

#[cfg(feature = "mtest")]
extern "C" fn {test_wrapper}() {{
    let test: fn() = {function};
    test()
}}

#[cfg(feature = "mtest")]
#[used]
#[unsafe(link_section = ".mtest_array")]
static {descriptor}: crate::__private_mtest::Descriptor =
    crate::__private_mtest::Descriptor::new(
        concat!(module_path!(), "::", stringify!({function})),
        {test_wrapper},
    );
"#
    )
}

/// Defines the unique safe firmware-policy entry.
///
/// The generated raw symbol is a fixed trampoline into the machine crate. The
/// generated Rust bridge enforces the exact `fn(machine::BootInfo) -> !` type
/// when the invoking firmware is compiled.
#[proc_macro]
pub fn entry(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();
    let Some(TokenTree::Ident(entry)) = tokens.next() else {
        return compile_error("expected one function name");
    };
    if tokens.next().is_some() {
        return compile_error("expected one function name");
    }

    expansion(&entry.to_string())
        .parse()
        .expect("the fixed entry expansion must remain valid Rust")
}

fn expansion(entry: &str) -> String {
    format!(
        r#"
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
    use super::{expansion, mtest_expansion};

    #[test]
    fn expansion_contains_one_raw_and_one_typed_entry() {
        let expanded = expansion("main");
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

    #[test]
    fn mtest_expansion_generates_name_and_registration() {
        let expanded = mtest_expansion("checks_timer", "fn checks_timer() {}");
        assert!(expanded.contains("let test: fn() = checks_timer;"));
        assert!(expanded.contains("concat!(module_path!(), \"::\", stringify!(checks_timer))"));
        assert!(expanded.contains("Descriptor::new"));
        assert!(expanded.contains("link_section = \".mtest_array\""));
        assert!(!expanded.contains("restore"));
    }
}
