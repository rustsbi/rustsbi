//! Internal implementation details of RustSBI macros.
//!
//! Do not use this crate directly.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{ToTokens, quote};
use syn::{
    Data, DeriveInput, GenericParam, Generics, Ident, Lifetime, LifetimeParam, Member,
    parse_macro_input,
};

#[derive(Clone)]
enum ParseMode {
    Static,
    Dynamic,
}

#[derive(Clone, Default)]
struct StaticImpl {
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

impl StaticImpl {
    fn replace_sbi_extension_ident(
        &mut self,
        extension_name: &str,
        member: Member,
    ) -> (bool, Option<Member>) {
        match extension_name {
            "rfnc" | "fence" => (true, self.fence.replace(member)),
            "hsm" => (true, self.hsm.replace(member)),
            "spi" | "ipi" => (true, self.ipi.replace(member)),
            "srst" | "reset" => (true, self.reset.replace(member)),
            "time" | "timer" => (true, self.timer.replace(member)),
            "pmu" => (true, self.pmu.replace(member)),
            "dbcn" | "console" => (true, self.console.replace(member)),
            "susp" => (true, self.susp.replace(member)),
            "cppc" => (true, self.cppc.replace(member)),
            "nacl" => (true, self.nacl.replace(member)),
            "sta" => (true, self.sta.replace(member)),
            "info" | "env_info" => (true, self.env_info.replace(member)),
            _ => (false, None),
        }
    }
}

#[derive(Clone, Default)]
struct DynamicImpl {
    fence: Vec<Member>,
    hsm: Vec<Member>,
    ipi: Vec<Member>,
    reset: Vec<Member>,
    timer: Vec<Member>,
    pmu: Vec<Member>,
    console: Vec<Member>,
    susp: Vec<Member>,
    cppc: Vec<Member>,
    nacl: Vec<Member>,
    sta: Vec<Member>,
    env_info: Option<Member>,
}

impl DynamicImpl {
    fn push_sbi_extension_ident(&mut self, extension_name: &str, member: Member) -> bool {
        match extension_name {
            "rfnc" | "fence" => self.fence.push(member),
            "hsm" => self.hsm.push(member),
            "spi" | "ipi" => self.ipi.push(member),
            "srst" | "reset" => self.reset.push(member),
            "time" | "timer" => self.timer.push(member),
            "pmu" => self.pmu.push(member),
            "dbcn" | "console" => self.console.push(member),
            "susp" => self.susp.push(member),
            "cppc" => self.cppc.push(member),
            "nacl" => self.nacl.push(member),
            "sta" => self.sta.push(member),
            "info" | "env_info" => return self.env_info.replace(member).is_none(),
            _ => return false,
        }
        true
    }
}

/// This macro should be used in `rustsbi` crate as `rustsbi::RustSBI`.
#[proc_macro_derive(RustSBI, attributes(rustsbi))]
pub fn derive_rustsbi(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let Data::Struct(strukt) = &input.data else {
        panic!("#[derive(RustSBI)] must be used on structs");
    };

    let mut ans = TokenStream::new();
    let mut parse_mode = ParseMode::Static;

    for attr in &input.attrs {
        if !attr.path().is_ident("rustsbi") {
            continue;
        }
        let parsed = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("dynamic") {
                parse_mode = ParseMode::Dynamic;
                Ok(())
            } else {
                let path = meta.path.to_token_stream().to_string().replace(' ', "");
                Err(meta.error(format_args!("unknown RustSBI struct attribute `{}`", path)))
            }
        });
        if let Err(err) = parsed {
            ans.extend(TokenStream::from(err.to_compile_error()));
        }
    }

    let mut static_impl = StaticImpl::default();
    let mut dynamic_impl = DynamicImpl::default();

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
                    match parse_mode {
                        ParseMode::Static => {
                            let (replaced, origin) = static_impl
                                .replace_sbi_extension_ident(extension_name, member.clone());
                            if replaced {
                                check_already_exists(field, extension_name, origin, &mut ans);
                                current_meta_accepted = true;
                            }
                        }
                        ParseMode::Dynamic => {
                            let replaced = dynamic_impl
                                .push_sbi_extension_ident(extension_name, member.clone());
                            if replaced {
                                current_meta_accepted = true;
                            }
                        }
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
            match parse_mode {
                ParseMode::Static => {
                    let (_replaced, origin) = static_impl
                        .replace_sbi_extension_ident(field_ident.to_string().as_str(), member);
                    check_already_exists(field, &field_ident.to_string(), origin, &mut ans);
                }
                ParseMode::Dynamic => {
                    let _replaced = dynamic_impl
                        .push_sbi_extension_ident(field_ident.to_string().as_str(), member);
                }
            }
        }
    }
    match parse_mode {
        ParseMode::Static => ans.extend(impl_derive_rustsbi_static(
            &input.ident,
            static_impl,
            &input.generics,
        )),
        ParseMode::Dynamic => ans.extend(impl_derive_rustsbi_dynamic(
            &input.ident,
            dynamic_impl,
            &input.generics,
        )),
    };
    ans
}

fn check_already_exists(
    field: &syn::Field,
    extension_name: &str,
    origin: Option<Member>,
    ans: &mut TokenStream,
) {
    if let Some(_origin) = origin {
        // TODO: provide more detailed proc macro error hinting that previous
        // definition of this extension resides in `origin` once RFC 1566
        // (Procedural Macro Diagnostics) is stabilized.
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
}

fn impl_derive_rustsbi_static(name: &Ident, imp: StaticImpl, generics: &Generics) -> TokenStream {
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
    let generated = quote! {
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
    generated.into()
}

fn impl_derive_rustsbi_dynamic(name: &Ident, imp: DynamicImpl, generics: &Generics) -> TokenStream {
    let mut fence_contents = quote! {};
    let mut prober_fence = quote! {};
    for fence in &imp.fence {
        fence_contents.extend(quote! {
            if ::rustsbi::_rustsbi_fence_probe(&self.#fence) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_fence(&self.#fence, param, function)
            }
        });
        prober_fence.extend(quote! {
            let value = ::rustsbi::_rustsbi_fence_probe(&self.0.#fence);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut timer_contents = quote! {};
    let mut prober_timer = quote! {};
    for timer in &imp.timer {
        timer_contents.extend(quote! {
            if ::rustsbi::_rustsbi_timer_probe(&self.#timer) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_timer(&self.#timer, param, function)
            }
        });
        prober_timer.extend(quote! {
            let value = ::rustsbi::_rustsbi_timer_probe(&self.0.#timer);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut ipi_contents = quote! {};
    let mut prober_ipi = quote! {};
    for ipi in &imp.ipi {
        ipi_contents.extend(quote! {
            if ::rustsbi::_rustsbi_ipi_probe(&self.#ipi) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_ipi(&self.#ipi, param, function)
            }
        });
        prober_ipi.extend(quote! {
            let value = ::rustsbi::_rustsbi_ipi_probe(&self.0.#ipi);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut hsm_contents = quote! {};
    let mut prober_hsm = quote! {};
    for hsm in &imp.hsm {
        hsm_contents.extend(quote! {
            if ::rustsbi::_rustsbi_hsm_probe(&self.#hsm) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_hsm(&self.#hsm, param, function)
            }
        });
        prober_hsm.extend(quote! {
            let value = ::rustsbi::_rustsbi_hsm_probe(&self.0.#hsm);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut reset_contents = quote! {};
    let mut prober_reset = quote! {};
    for reset in &imp.reset {
        reset_contents.extend(quote! {
            if ::rustsbi::_rustsbi_reset_probe(&self.#reset) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_reset(&self.#reset, param, function)
            }
        });
        prober_reset.extend(quote! {
            let value = ::rustsbi::_rustsbi_reset_probe(&self.0.#reset);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut pmu_contents = quote! {};
    let mut prober_pmu = quote! {};
    for pmu in &imp.pmu {
        pmu_contents.extend(quote! {
            if ::rustsbi::_rustsbi_pmu_probe(&self.#pmu) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_pmu(&self.#pmu, param, function)
            }
        });
        prober_pmu.extend(quote! {
            let value = ::rustsbi::_rustsbi_pmu_probe(&self.0.#pmu);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut console_contents = quote! {};
    let mut prober_console = quote! {};
    for console in &imp.console {
        console_contents.extend(quote! {
            if ::rustsbi::_rustsbi_console_probe(&self.#console) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_console(&self.#console, param, function)
            }
        });
        prober_console.extend(quote! {
            let value = ::rustsbi::_rustsbi_console_probe(&self.0.#console);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut susp_contents = quote! {};
    let mut prober_susp = quote! {};
    for susp in &imp.susp {
        susp_contents.extend(quote! {
            if ::rustsbi::_rustsbi_susp_probe(&self.#susp) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_susp(&self.#susp, param, function)
            }
        });
        prober_susp.extend(quote! {
            let value = ::rustsbi::_rustsbi_susp_probe(&self.0.#susp);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut cppc_contents = quote! {};
    let mut prober_cppc = quote! {};
    for cppc in &imp.cppc {
        cppc_contents.extend(quote! {
            if ::rustsbi::_rustsbi_cppc_probe(&self.#cppc) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_cppc(&self.#cppc, param, function)
            }
        });
        prober_cppc.extend(quote! {
            let value = ::rustsbi::_rustsbi_cppc_probe(&self.0.#cppc);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut nacl_contents = quote! {};
    let mut prober_nacl = quote! {};
    for nacl in &imp.nacl {
        nacl_contents.extend(quote! {
            if ::rustsbi::_rustsbi_nacl_probe(&self.#nacl) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_nacl(&self.#nacl, param, function)
            }
        });
        prober_nacl.extend(quote! {
            let value = ::rustsbi::_rustsbi_nacl_probe(&self.0.#nacl);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }
    let mut sta_contents = quote! {};
    let mut prober_sta = quote! {};
    for sta in &imp.sta {
        sta_contents.extend(quote! {
            if ::rustsbi::_rustsbi_sta_probe(&self.#sta) != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return ::rustsbi::_rustsbi_sta(&self.#sta, param, function)
            }
        });
        prober_sta.extend(quote! {
            let value = ::rustsbi::_rustsbi_sta_probe(&self.0.#sta);
            if value != ::rustsbi::spec::base::UNAVAILABLE_EXTENSION {
                return value
            }
        });
    }

    let (_, origin_ty_generics, _) = generics.split_for_impl();
    let prober_generics = {
        let mut ans = generics.clone();
        let lifetime = Lifetime::new("'_lt", Span::mixed_site());
        ans.params
            .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
        ans
    };
    let (impl_generics, ty_generics, where_clause) = prober_generics.split_for_impl();

    let define_prober = quote! {
        struct _Prober #impl_generics (&'_lt #name #origin_ty_generics) #where_clause;
        impl #impl_generics ::rustsbi::_ExtensionProbe for _Prober #ty_generics #where_clause {
            #[inline(always)]
            fn probe_extension(&self, extension: usize) -> usize {
                match extension {
                    ::rustsbi::spec::base::EID_BASE => 1,
                    ::rustsbi::spec::time::EID_TIME => { #prober_timer ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::spi::EID_SPI => { #prober_ipi ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::rfnc::EID_RFNC => { #prober_fence ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::srst::EID_SRST => { #prober_reset ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::hsm::EID_HSM => { #prober_hsm ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::pmu::EID_PMU => { #prober_pmu ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::dbcn::EID_DBCN => { #prober_console ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::susp::EID_SUSP => { #prober_susp ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::cppc::EID_CPPC => { #prober_cppc ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::nacl::EID_NACL => { #prober_nacl ::rustsbi::spec::base::UNAVAILABLE_EXTENSION },
                    ::rustsbi::spec::sta::EID_STA => { #prober_sta ::rustsbi::spec::base::UNAVAILABLE_EXTENSION}
                    _ => ::rustsbi::spec::base::UNAVAILABLE_EXTENSION,
                }
            }
        }
    };
    let base_result = if let Some(env_info) = imp.env_info {
        quote! {
            ::rustsbi::_rustsbi_base_env_info(param, function, &self.#env_info, prober)
        }
    } else {
        match () {
            #[cfg(not(feature = "machine"))]
            () => quote! {
                compile_error!(
                    "can't derive RustSBI: #[cfg(feature = \"machine\")] is needed to derive RustSBI with no extra `EnvInfo` provided; \
            consider adding an `info` parameter to provide machine environment information implementing `rustsbi::EnvInfo`\
            if RustSBI is not run on machine mode."
                )
            },
            #[cfg(feature = "machine")]
            () => quote! {
                ::rustsbi::_rustsbi_base_bare(param, function, prober)
            },
        }
    };
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let generated = quote! {
        impl #impl_generics ::rustsbi::RustSBI for #name #ty_generics #where_clause {
            #[inline]
            fn handle_ecall(&self, extension: usize, function: usize, param: [usize; 6]) -> ::rustsbi::SbiRet {
                match extension {
                    ::rustsbi::spec::rfnc::EID_RFNC => { #fence_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::time::EID_TIME => { #timer_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::spi::EID_SPI => { #ipi_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::hsm::EID_HSM => { #hsm_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::srst::EID_SRST => { #reset_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::pmu::EID_PMU => { #pmu_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::dbcn::EID_DBCN => { #console_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::susp::EID_SUSP => { #susp_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::cppc::EID_CPPC => { #cppc_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::nacl::EID_NACL => { #nacl_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::sta::EID_STA => { #sta_contents ::rustsbi::SbiRet::not_supported() },
                    ::rustsbi::spec::base::EID_BASE => {
                        #define_prober
                        let prober = _Prober(&self);
                        #base_result
                    }
                    _ => ::rustsbi::SbiRet::not_supported(),
                }
            }
        }
    };
    generated.into()
}
