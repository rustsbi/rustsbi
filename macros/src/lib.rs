use proc_macro::TokenStream;
use quote::quote;
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
    for field in &fields {
        // for attr in field.attrs {
        //     if let Meta::List(list) = attr.meta {
        //         let vars =
        //         Punctuated::<Ident, syn::Token![,]>::parse_terminated(
        //           &list.tokens,
        //         ).unwrap();
        //     }
        // }
        if let Some(name) = &field.ident {
            match name.to_string().as_str() {
                "rfnc" | "fence" => imp.fence = Some(name),
                "hsm" => imp.hsm = Some(name),
                "spi" | "ipi" => imp.ipi = Some(name),
                "srst" | "reset" => imp.reset = Some(name),
                "time" | "timer" => imp.timer = Some(name),
                "pmu" => imp.pmu = Some(name),
                "dbcn" | "console" => imp.console = Some(name),
                "susp" => imp.susp = Some(name),
                "cppc" => imp.cppc = Some(name),
                "nacl" => imp.nacl = Some(name),
                "sta" => imp.sta = Some(name),
                "info" | "env_info" => imp.env_info = Some(name),
                _ => {}
            }
        }
    }

    let mut ans = TokenStream::new();
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
    let fence_procedure = if let Some(fence) = &imp.fence {
        quote! {
            ::rustsbi::spec::rfnc::EID_RFNC => ::rustsbi::_rustsbi_fence(&self.#fence, param, function),
        }
    } else {
        quote! {}
    };
    let hsm_procedure = if let Some(hsm) = &imp.hsm {
        quote! {
            ::rustsbi::spec::hsm::EID_HSM => ::rustsbi::_rustsbi_hsm(&self.#hsm, param, function),
        }
    } else {
        quote! {}
    };
    let ipi_procedure = if let Some(ipi) = &imp.ipi {
        quote! {
            ::rustsbi::spec::spi::EID_SPI => ::rustsbi::_rustsbi_ipi(&self.#ipi, param, function),
        }
    } else {
        quote! {}
    };
    let reset_procedure = if let Some(reset) = &imp.reset {
        quote! {
            ::rustsbi::spec::srst::EID_SRST => ::rustsbi::_rustsbi_reset(&self.#reset, param, function),
        }
    } else {
        quote! {}
    };
    let timer_procedure = if let Some(timer) = &imp.timer {
        quote! {
            ::rustsbi::spec::time::EID_TIME => ::rustsbi::_rustsbi_timer(&self.#timer, param, function),
        }
    } else {
        quote! {}
    };
    let pmu_procedure = if let Some(pmu) = &imp.pmu {
        quote! {
            ::rustsbi::spec::pmu::EID_PMU => ::rustsbi::_rustsbi_pmu(&self.#pmu, param, function),
        }
    } else {
        quote! {}
    };
    let console_procedure = if let Some(console) = &imp.console {
        quote! {
            ::rustsbi::spec::dbcn::EID_DBCN => ::rustsbi::_rustsbi_console(&self.#console, param, function),
        }
    } else {
        quote! {}
    };
    let susp_procedure = if let Some(susp) = &imp.susp {
        quote! {
            ::rustsbi::spec::susp::EID_SUSP => ::rustsbi::_rustsbi_susp(&self.#susp, param, function),
        }
    } else {
        quote! {}
    };
    let cppc_procedure = if let Some(cppc) = &imp.cppc {
        quote! {
            ::rustsbi::spec::cppc::EID_CPPC => ::rustsbi::_rustsbi_cppc(&self.#cppc, param, function),
        }
    } else {
        quote! {}
    };
    let nacl_procedure = if let Some(nacl) = &imp.nacl {
        quote! {
            ::rustsbi::spec::nacl::EID_NACL => ::rustsbi::_rustsbi_nacl(&self.#nacl, param, function),
        }
    } else {
        quote! {}
    };
    let sta_procedure = if let Some(sta) = &imp.sta {
        quote! {
            ::rustsbi::spec::sta::EID_STA => ::rustsbi::_rustsbi_sta(&self.#sta, param, function),
        }
    } else {
        quote! {}
    };
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let gen = quote! {
    impl #impl_generics rustsbi::RustSBI for #name #ty_generics #where_clause {
        #[inline]
        fn handle_ecall(&self, extension: usize, function: usize, param: [usize; 6]) -> ::rustsbi::spec::binary::SbiRet {
            match extension {
                #fence_procedure
                #timer_procedure
                #ipi_procedure
                #base_procedure
                #hsm_procedure
                #reset_procedure
                #pmu_procedure
                #console_procedure
                #susp_procedure
                #cppc_procedure
                #nacl_procedure
                #sta_procedure
                _ => ::rustsbi::spec::binary::SbiRet::not_supported(),
            }
        }
    }
        };
    gen.into()
}
