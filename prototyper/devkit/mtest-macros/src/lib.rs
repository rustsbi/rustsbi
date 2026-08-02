//! Compile-time registration for the QEMU-virt M-test image.

use proc_macro::{TokenStream, TokenTree};

/// Declares one QEMU M-mode test in the dedicated link-time registry.
///
/// The test must have the shape `fn()`. Its stable name is generated from
/// `module_path!()` and the function name. A dedicated firmware invocation
/// devkit image selects and runs exactly one such test.
#[proc_macro_attribute]
pub fn mtest(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return compile_error("`mtest` does not accept arguments");
    }
    let function = match function_name(&item) {
        Ok(function) => function,
        Err(message) => return compile_error(&message),
    };
    mtest_expansion(&function, &item.to_string())
        .parse()
        .expect("the M-test expansion must remain valid Rust")
}

fn function_name(item: &TokenStream) -> Result<String, String> {
    let mut tokens = item.clone().into_iter();
    loop {
        let Some(token) = tokens.next() else {
            return Err("`mtest` can be applied only to a function".into());
        };
        if matches!(&token, TokenTree::Ident(ident) if ident.to_string() == "fn") {
            let Some(TokenTree::Ident(function)) = tokens.next() else {
                return Err("expected a function name after `fn` in `mtest`".into());
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
{item}

extern "C" fn {test_wrapper}() {{
    let test: fn() = {function};
    test()
}}

#[used]
#[unsafe(link_section = ".mtest_array")]
static {descriptor}: crate::Descriptor =
    crate::Descriptor::new(
        concat!(module_path!(), "::", stringify!({function})),
        {test_wrapper},
    );
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
    use super::mtest_expansion;

    #[test]
    fn mtest_expansion_generates_name_and_registration() {
        let expanded = mtest_expansion("checks_timer", "fn checks_timer() {}");
        assert!(expanded.contains("let test: fn() = checks_timer;"));
        assert!(expanded.contains("concat!(module_path!(), \"::\", stringify!(checks_timer))"));
        assert!(expanded.contains("crate::Descriptor::new"));
        assert!(expanded.contains("link_section = \".mtest_array\""));
        assert!(!expanded.contains("restore"));
    }
}
