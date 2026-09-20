use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{ToTokens, quote};
use syn::{Data, DeriveInput, Generics, Ident, Member};

use crate::extension;

pub(crate) fn derive(input: DeriveInput) -> TokenStream {
    let mut ans = TokenStream::new();
    let mut crate_path: Option<syn::Path> = None;

    for attr in &input.attrs {
        if !attr.path().is_ident("rustsbi") {
            continue;
        }
        let parsed = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                let path: syn::Path = meta.value()?.parse()?;
                if crate_path.replace(path).is_some() {
                    Err(meta.error("`crate` may only be specified once"))
                } else {
                    Ok(())
                }
            } else {
                let path = meta.path.to_token_stream().to_string().replace(' ', "");
                Err(meta.error(format_args!("unknown VendorSBI type attribute `{path}`")))
            }
        });
        if let Err(err) = parsed {
            ans.extend(TokenStream::from(err.to_compile_error()));
        }
    }

    let krate = match &crate_path {
        Some(path) => path.to_token_stream(),
        None => quote! { ::rustsbi },
    };

    match &input.data {
        Data::Struct(strukt) => {
            let mut extensions = Vec::new();
            for (i, field) in strukt.fields.iter().enumerate() {
                let member = match &field.ident {
                    Some(ident) => Member::Named(ident.clone()),
                    None => Member::Unnamed(i.into()),
                };
                for attr in &field.attrs {
                    if !attr.path().is_ident("rustsbi") {
                        continue;
                    }
                    let parsed = attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("extension") {
                            extensions.push(extension::Extension::parse(&meta, member.clone())?);
                            Ok(())
                        } else if meta.path.is_ident("skip") {
                            Ok(())
                        } else {
                            let path = meta.path.to_token_stream().to_string().replace(' ', "");
                            Err(meta
                                .error(format_args!("unknown VendorSBI field attribute `{path}`")))
                        }
                    });
                    if let Err(err) = parsed {
                        ans.extend(TokenStream::from(err.to_compile_error()));
                    }
                }
            }
            ans.extend(TokenStream::from(extension::validate(&extensions, &krate)));
            ans.extend(derive_struct(
                &input.ident,
                &krate,
                &input.generics,
                &extensions,
            ));
            let body = proc_macro2::TokenStream::from(ans);
            quote! { const _: () = { #body }; }.into()
        }
        Data::Enum(enumeration) => {
            let mut variants = Vec::new();
            for variant in &enumeration.variants {
                match &variant.fields {
                    syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                        variants.push(&variant.ident);
                    }
                    _ => ans.extend(TokenStream::from(
                        syn::Error::new_spanned(
                            variant,
                            "VendorSBI enum variants must contain exactly one unnamed field",
                        )
                        .to_compile_error(),
                    )),
                }
            }
            if !ans.is_empty() {
                return ans;
            }
            let probe_arms = variants.iter().map(|name| {
                quote! {
                    Self::#name(inner) => #krate::VendorSBI::probe_extension(inner, extension),
                }
            });
            let handle_arms = variants.iter().map(|name| quote! {
                Self::#name(inner) => #krate::VendorSBI::handle_ecall(inner, extension, function, args),
            });
            let name = &input.ident;
            let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
            quote! {
                impl #impl_generics #krate::VendorSBI for #name #ty_generics #where_clause {
                    #[inline]
                    fn probe_extension(&self, extension: usize) -> usize {
                        match self { #(#probe_arms)* }
                    }

                    #[inline]
                    fn handle_ecall(
                        &self,
                        extension: usize,
                        function: usize,
                        args: [usize; 6],
                    ) -> #krate::SbiRet {
                        match self { #(#handle_arms)* }
                    }
                }
            }
            .into()
        }
        Data::Union(_) => syn::Error::new_spanned(
            input,
            "#[derive(VendorSBI)] must be used on structs or enums",
        )
        .to_compile_error()
        .into(),
    }
}

fn derive_struct(
    name: &Ident,
    krate: &proc_macro2::TokenStream,
    generics: &Generics,
    extensions: &[extension::Extension],
) -> TokenStream {
    let extension = Ident::new("__rustsbi_extension", Span::mixed_site());
    let function = Ident::new("__rustsbi_function", Span::mixed_site());
    let param = Ident::new("__rustsbi_param", Span::mixed_site());
    let probe = extension::probe(extensions, krate, quote! { self });
    let dispatch = extension::dispatch(extensions, krate);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    quote! {
        impl #impl_generics #krate::VendorSBI for #name #ty_generics #where_clause {
            #[inline]
            fn probe_extension(&self, #extension: usize) -> usize {
                match #extension {
                    #probe
                    _ => 0,
                }
            }

            #[inline]
            fn handle_ecall(
                &self,
                #extension: usize,
                #function: usize,
                #param: [usize; 6],
            ) -> #krate::SbiRet {
                match #extension {
                    #dispatch
                    _ => #krate::SbiRet::not_supported(),
                }
            }
        }
    }
    .into()
}
