use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, punctuated::Punctuated, Data, DeriveInput, Fields, Ident};

#[derive(Default)]
struct RustSBIImp {
    fence: Option<Ident>,
    hsm: Option<Ident>,
    ipi: Option<Ident>,
    reset: Option<Ident>,
    timer: Option<Ident>,
    pmu: Option<Ident>,
    console: Option<Ident>,
    susp: Option<Ident>,
    cppc: Option<Ident>,
}

/// Implement RustSBI trait for structure of each extensions.
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
    for field in fields {
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
                "rfnc" | "fence" => imp.fence = Some(name.clone()),
                "hsm" => imp.hsm = Some(name.clone()),
                "spi" | "ipi" => imp.ipi = Some(name.clone()),
                "srst" | "reset" => imp.reset = Some(name.clone()),
                "timer" => imp.timer = Some(name.clone()),
                "pmu" => imp.pmu = Some(name.clone()),
                "dbcn" | "console" => imp.console = Some(name.clone()),
                "susp" => imp.susp = Some(name.clone()),
                "cppc" => imp.cppc = Some(name.clone()),
                _ => {}
            }
        }
    }

    let mut ans = TokenStream::new();
    ans.extend(impl_derive_rustsbi(&input.ident, &imp));
    ans
}

fn impl_derive_rustsbi(name: &Ident, imp: &RustSBIImp) -> TokenStream {
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
    let base_procedure = quote! {
        ::rustsbi::spec::base::EID_BASE => ::rustsbi::_rustsbi_base_machine(param, function,
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
            }),
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
            ::rustsbi::spec::timer::EID_TIMER => ::rustsbi::_rustsbi_timer(&self.#timer, param, function),
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
    let gen = quote! {
    impl rustsbi::RustSBI for #name {
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
                _ => SbiRet::not_supported(),
            }
        }
    }
        };
    gen.into()
}
