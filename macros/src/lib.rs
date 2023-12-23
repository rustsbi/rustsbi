use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, punctuated::Punctuated, Data, DeriveInput, Fields, Generics, Ident};

#[derive(Clone, Default)]
struct RustSBIImp<'a> {
    fence: Option<&'a Ident>,
    hsm: Option<&'a Ident>,
    ipi: Option<&'a Ident>,
    reset: Option<&'a Ident>,
    timer: Option<&'a Ident>,
    pmu: Option<&'a Ident>,
    console: Option<&'a Ident>,
    susp: Option<&'a Ident>,
    cppc: Option<&'a Ident>,
    nacl: Option<&'a Ident>,
    sta: Option<&'a Ident>,
    env_info: Option<&'a Ident>,
}

/// This macro should be used in `rustsbi` crate as `rustsbi::RustSBI`.
#[proc_macro_derive(RustSBI, attributes(rustsbi))]
pub fn derive_rustsbi(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let Data::Struct(strukt) = input.data else {
        panic!("#[derive(RustSBI)] must be used on structs");
    };

    let fields = match strukt.fields {
        Fields::Named(f) => f.named,
        Fields::Unnamed(f) => f.unnamed,
        Fields::Unit => Punctuated::new(),
    };
    let mut imp = RustSBIImp::default();

    let mut ans = TokenStream::new();

    for field in &fields {
        let mut skipped = false;
        for attr in &field.attrs {
            if !attr.path().is_ident("rustsbi") {
                continue;
            }
            let parsed = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    // skip this field in RustSBI
                    skipped = true;
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
        if skipped {
            continue;
        }
        if let Some(name) = &field.ident {
            let origin = match name.to_string().as_str() {
                "rfnc" | "fence" => imp.fence.replace(name),
                "hsm" => imp.hsm.replace(name),
                "spi" | "ipi" => imp.ipi.replace(name),
                "srst" | "reset" => imp.reset.replace(name),
                "time" | "timer" => imp.timer.replace(name),
                "pmu" => imp.pmu.replace(name),
                "dbcn" | "console" => imp.console.replace(name),
                "susp" => imp.susp.replace(name),
                "cppc" => imp.cppc.replace(name),
                "nacl" => imp.nacl.replace(name),
                "sta" => imp.sta.replace(name),
                "info" | "env_info" => imp.env_info.replace(name),
                _ => continue,
            };
            if let Some(_origin) = origin {
                // TODO: provide more detailed proc macro error hinting that previous
                // definition of this extension resides in `origin` once RFC 1566
                // (Procedural Macro Diagnostics) is stablized.
                // Link: https://github.com/rust-lang/rust/issues/54140
                let error = syn::Error::new_spanned(
                    &field,
                    format!("more than one field defined SBI extension '{}'. \
                    At most one fields should define the same SBI extension; consider using \
                    #[rustsbi(skip)] to ignore fields that shouldn't be treated as an extension.", name),
                );
                ans.extend(TokenStream::from(error.to_compile_error()));
            }
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
            consider adding an `info` parameter to provide machine information implementing `rustsbi::EnvInfo`\
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
        fn handle_ecall(&self, extension: usize, function: usize, param: [usize; 6]) -> ::rustsbi::spec::binary::SbiRet {
            match extension {
                #match_arms
                _ => ::rustsbi::spec::binary::SbiRet::not_supported(),
            }
        }
    }
        };
    gen.into()
}
