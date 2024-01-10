//! Internal implementation details of RustSBI macros.
//!
//! Do not use this crate directly.

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Data, DeriveInput, Generics, Ident, Member};

#[derive(Clone, Default)]
struct RustSBIImp {
    fence: Option<Member>,
    hsm: Option<Member>,
    ipi: Option<Member>,
    reset: Option<Member>,
    timer: Option<Member>,
    pmu: Option<Member>,
    console: Option<Member>,
    susp: Option<Member>,
    cppc: Option<Member>,
    nacl: Option<Member>,
    sta: Option<Member>,
    env_info: Option<Member>,
}

/// This macro should be used in `rustsbi` crate as `rustsbi::RustSBI`.
#[proc_macro_derive(RustSBI, attributes(rustsbi))]
pub fn derive_rustsbi(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let Data::Struct(strukt) = input.data else {
        panic!("#[derive(RustSBI)] must be used on structs");
    };

    let mut imp = RustSBIImp::default();

    let mut replace_sbi_extension_ident =
        |extension_name: &str, member: Member| match extension_name {
            "rfnc" | "fence" => (true, imp.fence.replace(member)),
            "hsm" => (true, imp.hsm.replace(member)),
            "spi" | "ipi" => (true, imp.ipi.replace(member)),
            "srst" | "reset" => (true, imp.reset.replace(member)),
            "time" | "timer" => (true, imp.timer.replace(member)),
            "pmu" => (true, imp.pmu.replace(member)),
            "dbcn" | "console" => (true, imp.console.replace(member)),
            "susp" => (true, imp.susp.replace(member)),
            "cppc" => (true, imp.cppc.replace(member)),
            "nacl" => (true, imp.nacl.replace(member)),
            "sta" => (true, imp.sta.replace(member)),
            "info" | "env_info" => (true, imp.env_info.replace(member)),
            _ => (false, None),
        };
    let mut ans = TokenStream::new();
    let check_already_exists = |field: &syn::Field,
                                extension_name: &str,
                                origin: Option<Member>,
                                ans: &mut TokenStream| {
        if let Some(_origin) = origin {
            // TODO: provide more detailed proc macro error hinting that previous
            // definition of this extension resides in `origin` once RFC 1566
            // (Procedural Macro Diagnostics) is stablized.
            // Link: https://github.com/rust-lang/rust/issues/54140
            let error = syn::Error::new_spanned(
                field,
                format!(
                    "more than one field defined SBI extension '{}'. \
                At most one fields should define the same SBI extension; consider using \
                #[rustsbi(skip)] to ignore fields that shouldn't be treated as an extension.",
                    extension_name
                ),
            );
            ans.extend(TokenStream::from(error.to_compile_error()));
        }
    };

    for (i, field) in strukt.fields.iter().enumerate() {
        let member = match &field.ident {
            Some(ident) => Member::Named(ident.clone()),
            None => Member::Unnamed(i.into()),
        };
        let mut field_already_parsed = false;
        for attr in &field.attrs {
            if !attr.path().is_ident("rustsbi") {
                continue;
            }
            let parsed = attr.parse_nested_meta(|meta| {
                let mut current_meta_accepted = false;
                if meta.path.is_ident("skip") {
                    // accept meta but do nothing, effectively skip this field in RustSBI
                    current_meta_accepted = true;
                } else if let Some(meta_path_ident) = meta.path.get_ident() {
                    let extension_name = &meta_path_ident.to_string();
                    let (replaced, origin) =
                        replace_sbi_extension_ident(extension_name, member.clone());
                    if replaced {
                        check_already_exists(field, extension_name, origin, &mut ans);
                        current_meta_accepted = true;
                    }
                }
                if current_meta_accepted {
                    field_already_parsed = true;
                    Ok(())
                } else {
                    let path = meta.path.to_token_stream().to_string().replace(' ', "");
                    Err(meta.error(format_args!("unknown RustSBI variant attribute `{}`", path)))
                }
            });
            if let Err(err) = parsed {
                ans.extend(TokenStream::from(err.to_compile_error()));
            }
        }
        // Already parsed by inner attribute.
        // Could be either skipped using #[rustsbi(skip)], or renamed using #[rustsbi(some_extension)]
        if field_already_parsed {
            continue;
        }
        if let Some(field_ident) = &field.ident {
            let (_replaced, origin) =
                replace_sbi_extension_ident(field_ident.to_string().as_str(), member);
            check_already_exists(field, &field_ident.to_string(), origin, &mut ans);
        }
    }

    ans.extend(impl_derive_rustsbi(&input.ident, imp, &input.generics));
    ans
}

fn impl_derive_rustsbi(name: &Ident, imp: RustSBIImp, generics: &Generics) -> TokenStream {
    let base_probe: usize = 1;
    let fence_probe: usize = if imp.fence.is_some() { 1 } else { 0 };
    let hsm_probe: usize = if imp.hsm.is_some() { 1 } else { 0 };
    let ipi_probe: usize = if imp.ipi.is_some() { 1 } else { 0 };
    let reset_probe: usize = if imp.reset.is_some() { 1 } else { 0 };
    let timer_probe: usize = if imp.timer.is_some() { 1 } else { 0 };
    let pmu_probe: usize = if imp.pmu.is_some() { 1 } else { 0 };
    let console_probe: usize = if imp.console.is_some() { 1 } else { 0 };
    let susp_probe: usize = if imp.susp.is_some() { 1 } else { 0 };
    let cppc_probe: usize = if imp.cppc.is_some() { 1 } else { 0 };
    let nacl_probe: usize = if imp.nacl.is_some() { 1 } else { 0 };
    let sta_probe: usize = if imp.sta.is_some() { 1 } else { 0 };
    let probe = quote! {
        ::rustsbi::_StandardExtensionProbe {
            base: #base_probe,
            fence: #fence_probe,
            hsm: #hsm_probe,
            ipi: #ipi_probe,
            reset: #reset_probe,
            timer: #timer_probe,
            pmu: #pmu_probe,
            console: #console_probe,
            susp: #susp_probe,
            cppc: #cppc_probe,
            nacl: #nacl_probe,
            sta: #sta_probe,
        }
    };
    let mut match_arms = quote! {};
    let base_procedure = if let Some(env_info) = imp.env_info {
        quote! {
            ::rustsbi::spec::base::EID_BASE => ::rustsbi::_rustsbi_base_env_info(param, function, &self.#env_info, #probe),
        }
    } else {
        match () {
            #[cfg(not(feature = "machine"))]
            () => quote! {
                ::rustsbi::spec::base::EID_BASE => compile_error!(
                    "can't derive RustSBI: #[cfg(feature = \"machine\")] is needed to derive RustSBI with no extra `EnvInfo` provided; \
            consider adding an `info` parameter to provide machine environment information implementing `rustsbi::EnvInfo`\
            if RustSBI is not run on machine mode."
                ),
            },
            #[cfg(feature = "machine")]
            () => quote! {
                ::rustsbi::spec::base::EID_BASE => ::rustsbi::_rustsbi_base_bare(param, function, #probe),
            },
        }
    };
    match_arms.extend(base_procedure);
    if let Some(fence) = &imp.fence {
        match_arms.extend(quote! {
            ::rustsbi::spec::rfnc::EID_RFNC => ::rustsbi::_rustsbi_fence(&self.#fence, param, function),
        })
    };
    if let Some(timer) = &imp.timer {
        match_arms.extend(quote! {
            ::rustsbi::spec::time::EID_TIME => ::rustsbi::_rustsbi_timer(&self.#timer, param, function),
        })
    };
    if let Some(ipi) = &imp.ipi {
        match_arms.extend(quote! {
            ::rustsbi::spec::spi::EID_SPI => ::rustsbi::_rustsbi_ipi(&self.#ipi, param, function),
        })
    }
    if let Some(hsm) = &imp.hsm {
        match_arms.extend(quote! {
            ::rustsbi::spec::hsm::EID_HSM => ::rustsbi::_rustsbi_hsm(&self.#hsm, param, function),
        })
    }
    if let Some(reset) = &imp.reset {
        match_arms.extend(quote! {
            ::rustsbi::spec::srst::EID_SRST => ::rustsbi::_rustsbi_reset(&self.#reset, param, function),
        })
    }
    if let Some(pmu) = &imp.pmu {
        match_arms.extend(quote! {
            ::rustsbi::spec::pmu::EID_PMU => ::rustsbi::_rustsbi_pmu(&self.#pmu, param, function),
        })
    }
    if let Some(console) = &imp.console {
        match_arms.extend(quote! {
            ::rustsbi::spec::dbcn::EID_DBCN => ::rustsbi::_rustsbi_console(&self.#console, param, function),
        })
    }
    if let Some(susp) = &imp.susp {
        match_arms.extend(quote! {
            ::rustsbi::spec::susp::EID_SUSP => ::rustsbi::_rustsbi_susp(&self.#susp, param, function),
        })
    }
    if let Some(cppc) = &imp.cppc {
        match_arms.extend(quote! {
            ::rustsbi::spec::cppc::EID_CPPC => ::rustsbi::_rustsbi_cppc(&self.#cppc, param, function),
        })
    }
    if let Some(nacl) = &imp.nacl {
        match_arms.extend(quote! {
            ::rustsbi::spec::nacl::EID_NACL => ::rustsbi::_rustsbi_nacl(&self.#nacl, param, function),
        })
    }
    if let Some(sta) = &imp.sta {
        match_arms.extend(quote! {
            ::rustsbi::spec::sta::EID_STA => ::rustsbi::_rustsbi_sta(&self.#sta, param, function),
        })
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let gen = quote! {
    impl #impl_generics ::rustsbi::RustSBI for #name #ty_generics #where_clause {
        #[inline]
        fn handle_ecall(&self, extension: usize, function: usize, param: [usize; 6]) -> ::rustsbi::SbiRet {
            match extension {
                #match_arms
                _ => ::rustsbi::SbiRet::not_supported(),
            }
        }
    }
        };
    gen.into()
}
