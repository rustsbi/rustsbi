use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Expr, Lit, Member, meta::ParseNestedMeta};

/// One explicitly bound EID, shared by call dispatch and BASE probing.
pub(crate) struct Extension {
    eid: Expr,
    member: Member,
}

impl Extension {
    pub(crate) fn parse(meta: &ParseNestedMeta<'_>, member: Member) -> syn::Result<Self> {
        let mut eid = None;
        meta.parse_nested_meta(|meta| {
            if !meta.path.is_ident("eid") {
                return Err(meta.error("expected `eid = <integer or constant path>`"));
            }
            let value: Expr = meta.value()?.parse()?;
            if !matches!(
                &value,
                Expr::Path(_)
                    | Expr::Lit(syn::ExprLit {
                        lit: Lit::Int(_),
                        ..
                    })
            ) {
                return Err(meta.error("EID must be an integer literal or a constant path"));
            }
            if eid.replace(value).is_some() {
                return Err(meta.error("`eid` may only be specified once"));
            }
            Ok(())
        })?;
        let eid = eid.ok_or_else(|| meta.error("missing `eid` for vendor extension"))?;
        Ok(Self { eid, member })
    }
}

fn binding(index: usize) -> syn::Ident {
    format_ident!("__RUSTSBI_VENDOR_EID_{index}", span = Span::mixed_site())
}

pub(crate) fn validate(extensions: &[Extension], krate: &TokenStream) -> TokenStream {
    if extensions.is_empty() {
        return quote! {};
    }
    let bindings = extensions.iter().enumerate().map(|(index, extension)| {
        let name = binding(index);
        let eid = &extension.eid;
        quote! { const #name: usize = #eid; }
    });
    let ids = (0..extensions.len()).map(binding);
    quote! {
        #(#bindings)*
        const _: () = {
            let ids: &[usize] = &[#(#ids),*];
            let mut i = 0;
            while i < ids.len() {
                assert!(ids[i] <= u32::MAX as usize, "vendor EID must fit in 32 bits");
                assert!(!matches!(ids[i],
                    0..=8 |
                    #krate::spec::base::EID_BASE |
                    #krate::spec::time::EID_TIME |
                    #krate::spec::spi::EID_SPI |
                    #krate::spec::rfnc::EID_RFNC |
                    #krate::spec::hsm::EID_HSM |
                    #krate::spec::srst::EID_SRST |
                    #krate::spec::pmu::EID_PMU |
                    #krate::spec::dbcn::EID_DBCN |
                    #krate::spec::susp::EID_SUSP |
                    #krate::spec::cppc::EID_CPPC |
                    #krate::spec::nacl::EID_NACL |
                    #krate::spec::sta::EID_STA |
                    #krate::spec::mpxy::EID_MPXY |
                    #krate::spec::dbtr::EID_DBTR |
                    #krate::spec::fwft::EID_FWFT |
                    #krate::spec::sse::EID_SSE
                ), "vendor EID conflicts with a standard extension");
                let mut j = 0;
                while j < i {
                    assert!(ids[i] != ids[j], "duplicate vendor EID");
                    j += 1;
                }
                i += 1;
            }
        };
    }
}

pub(crate) fn dispatch(extensions: &[Extension], krate: &TokenStream) -> TokenStream {
    let function = syn::Ident::new("__rustsbi_function", Span::mixed_site());
    let param = syn::Ident::new("__rustsbi_param", Span::mixed_site());
    let arms = extensions
        .iter()
        .enumerate()
        .map(|(index, Extension { member, .. })| {
            let eid = binding(index);
            quote! {
                #eid => {
                    if #krate::Extension::probe(&self.#member) == 0 {
                        #krate::SbiRet::not_supported()
                    } else {
                        #krate::Extension::handle(&self.#member, #function, #param)
                    }
                },
            }
        });
    quote! { #(#arms)* }
}

pub(crate) fn probe(
    extensions: &[Extension],
    krate: &TokenStream,
    receiver: TokenStream,
) -> TokenStream {
    let arms = extensions
        .iter()
        .enumerate()
        .map(|(index, Extension { member, .. })| {
            let eid = binding(index);
            quote! { #eid => #krate::Extension::probe(&#receiver.#member), }
        });
    quote! { #(#arms)* }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn parse(attr: syn::Attribute) -> syn::Result<()> {
        attr.parse_nested_meta(|meta| {
            Extension::parse(&meta, parse_quote!(custom))?;
            Ok(())
        })
    }

    #[test]
    fn parse_eid() {
        assert!(parse(parse_quote!(#[rustsbi(extension(eid = 0x0900_031e))])).is_ok());
        assert!(parse(parse_quote!(#[rustsbi(extension(eid = vendor::EID))])).is_ok());
    }

    #[test]
    fn reject_invalid_attributes() {
        for (attr, expected) in [
            (
                parse_quote!(#[rustsbi(extension())]),
                "expected nested attribute",
            ),
            (
                parse_quote!(#[rustsbi(extension(eid = 1, eid = 2))]),
                "only be specified once",
            ),
            (parse_quote!(#[rustsbi(extension(id = 1))]), "expected `eid"),
            (
                parse_quote!(#[rustsbi(extension(eid = -1))]),
                "integer literal or a constant path",
            ),
            (
                parse_quote!(#[rustsbi(extension(eid = "vendor"))]),
                "integer literal or a constant path",
            ),
        ] {
            let error = parse(attr).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}; expected {expected}");
        }
    }
}
