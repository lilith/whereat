//! Derive macros for errat error tracing.
//!
//! This crate provides `#[derive(TracedError)]` for automatic error type setup.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Derive macro for creating traced error types.
///
/// Generates:
/// - `impl Display` using `#[error("...")]` format strings
/// - `impl ErrorMeta` using `#[errat(...)]` attributes
/// - `impl From<T> for Traced<Self>` for variants with `#[from]`
///
/// ## Attributes
///
/// ### Type-level `#[errat(...)]`
///
/// - `repo = "url"` - GitHub repository URL for clickable links
/// - `crate_name = "name"` - Override crate name (defaults to `env!("CARGO_PKG_NAME")`)
/// - `docs = "url"` - Documentation URL
/// - `commit = "hash"` - Git commit (defaults to compile-time env var if set)
///
/// ### Variant-level
///
/// - `#[error("format string")]` - Display format for this variant
/// - `#[from]` - Generate `From` impl for the inner type
///
/// ## Example
///
/// ```ignore
/// use errat::TracedError;
///
/// #[derive(Debug, TracedError)]
/// #[errat(repo = "https://github.com/user/repo")]
/// enum MyError {
///     #[error("not found: {0}")]
///     NotFound(String),
///
///     #[error("io error: {0}")]
///     #[from]
///     Io(std::io::Error),
/// }
/// ```
#[proc_macro_derive(TracedError, attributes(errat, error, from))]
pub fn derive_traced_error(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match derive_traced_error_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_traced_error_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Parse type-level #[errat(...)] attributes
    let errat_attrs = parse_errat_attrs(&input.attrs)?;

    // Only handle enums for now
    let data_enum = match &input.data {
        Data::Enum(e) => e,
        Data::Struct(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "TracedError can only be derived for enums (struct support coming soon)",
            ));
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "TracedError cannot be derived for unions",
            ));
        }
    };

    // Generate Display match arms
    let display_arms = generate_display_arms(data_enum)?;

    // Generate From impls for #[from] variants
    let from_impls =
        generate_from_impls(name, &impl_generics, &ty_generics, where_clause, data_enum)?;

    // Generate ErrorMeta impl
    let error_meta_impl = generate_error_meta_impl(
        name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &errat_attrs,
    );

    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #display_arms
                }
            }
        }

        #error_meta_impl

        #from_impls
    })
}

/// Parsed #[errat(...)] attributes
#[derive(Default)]
struct ErratAttrs {
    repo: Option<String>,
    crate_name: Option<String>,
    docs: Option<String>,
    commit: Option<String>,
}

fn parse_errat_attrs(attrs: &[syn::Attribute]) -> syn::Result<ErratAttrs> {
    let mut result = ErratAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("errat") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("repo") {
                let value: syn::LitStr = meta.value()?.parse()?;
                result.repo = Some(value.value());
            } else if meta.path.is_ident("crate_name") {
                let value: syn::LitStr = meta.value()?.parse()?;
                result.crate_name = Some(value.value());
            } else if meta.path.is_ident("docs") {
                let value: syn::LitStr = meta.value()?.parse()?;
                result.docs = Some(value.value());
            } else if meta.path.is_ident("commit") {
                let value: syn::LitStr = meta.value()?.parse()?;
                result.commit = Some(value.value());
            }
            Ok(())
        })?;
    }

    Ok(result)
}

fn generate_display_arms(data_enum: &syn::DataEnum) -> syn::Result<TokenStream2> {
    let mut arms = TokenStream2::new();

    for variant in &data_enum.variants {
        let variant_name = &variant.ident;

        // Find #[error("...")] attribute
        let error_msg = find_error_attr(&variant.attrs)?;

        let (pattern, format_impl) = match &variant.fields {
            Fields::Unit => {
                let msg = error_msg.unwrap_or_else(|| variant_name.to_string());
                (quote! { Self::#variant_name }, quote! { write!(f, #msg) })
            }
            Fields::Unnamed(fields) => {
                let field_count = fields.unnamed.len();
                let field_names: Vec<_> = (0..field_count)
                    .map(|i| syn::Ident::new(&format!("_{}", i), variant_name.span()))
                    .collect();

                let pattern = quote! { Self::#variant_name(#(#field_names),*) };

                if let Some(msg) = error_msg {
                    // Parse format string and replace {0}, {1}, etc.
                    let format_impl = generate_format_call(&msg, &field_names);
                    (pattern, format_impl)
                } else if field_count == 1 {
                    // Default: just display the inner value
                    (pattern, quote! { write!(f, "{}", _0) })
                } else {
                    // Default: variant name
                    let msg = variant_name.to_string();
                    (pattern, quote! { write!(f, #msg) })
                }
            }
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| f.ident.as_ref().unwrap())
                    .collect();

                let pattern = quote! { Self::#variant_name { #(#field_names),* } };

                if let Some(msg) = error_msg {
                    let format_impl = generate_named_format_call(&msg, &field_names);
                    (pattern, format_impl)
                } else {
                    let msg = variant_name.to_string();
                    (pattern, quote! { write!(f, #msg) })
                }
            }
        };

        arms.extend(quote! {
            #pattern => #format_impl,
        });
    }

    Ok(arms)
}

fn find_error_attr(attrs: &[syn::Attribute]) -> syn::Result<Option<String>> {
    for attr in attrs {
        if !attr.path().is_ident("error") {
            continue;
        }

        // Parse #[error("message")]
        let args: syn::LitStr = attr.parse_args()?;
        return Ok(Some(args.value()));
    }
    Ok(None)
}

fn generate_format_call(format_str: &str, field_names: &[syn::Ident]) -> TokenStream2 {
    // Simple approach: just use write! with the format string and fields in order
    // The format string should use {0}, {1}, etc. or just {}
    quote! {
        write!(f, #format_str, #(#field_names),*)
    }
}

fn generate_named_format_call(format_str: &str, field_names: &[&syn::Ident]) -> TokenStream2 {
    // For named fields, use named arguments in write!
    quote! {
        write!(f, #format_str, #(#field_names = #field_names),*)
    }
}

fn generate_from_impls(
    enum_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics<'_>,
    ty_generics: &syn::TypeGenerics<'_>,
    where_clause: Option<&syn::WhereClause>,
    data_enum: &syn::DataEnum,
) -> syn::Result<TokenStream2> {
    let mut impls = TokenStream2::new();

    for variant in &data_enum.variants {
        // Check for #[from] attribute
        let has_from = variant.attrs.iter().any(|a| a.path().is_ident("from"));
        if !has_from {
            continue;
        }

        let variant_name = &variant.ident;

        // Get the inner type (must be a single unnamed field)
        let inner_ty = match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                &fields.unnamed.first().unwrap().ty
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "#[from] can only be used on variants with a single unnamed field",
                ));
            }
        };

        // Generate From<InnerType> for EnumType (like thiserror does)
        impls.extend(quote! {
            impl #impl_generics ::core::convert::From<#inner_ty> for #enum_name #ty_generics #where_clause {
                fn from(err: #inner_ty) -> Self {
                    #enum_name::#variant_name(err)
                }
            }
        });
    }

    Ok(impls)
}

fn generate_error_meta_impl(
    name: &syn::Ident,
    impl_generics: &syn::ImplGenerics<'_>,
    ty_generics: &syn::TypeGenerics<'_>,
    where_clause: Option<&syn::WhereClause>,
    attrs: &ErratAttrs,
) -> TokenStream2 {
    let crate_name_impl = if let Some(ref crate_name) = attrs.crate_name {
        quote! { Some(#crate_name) }
    } else {
        quote! { Some(env!("CARGO_PKG_NAME")) }
    };

    let repo_url_impl = if let Some(ref repo) = attrs.repo {
        quote! { Some(#repo) }
    } else {
        quote! { option_env!("CARGO_PKG_REPOSITORY") }
    };

    let docs_url_impl = if let Some(ref docs) = attrs.docs {
        quote! { Some(#docs) }
    } else {
        quote! { None }
    };

    let git_commit_impl = if let Some(ref commit) = attrs.commit {
        quote! { Some(#commit) }
    } else {
        // Try common env vars for git commit
        quote! {
            option_env!("GIT_COMMIT")
                .or(option_env!("GITHUB_SHA"))
                .or(option_env!("CI_COMMIT_SHA"))
        }
    };

    quote! {
        impl #impl_generics ::errat::ErrorMeta for #name #ty_generics #where_clause {
            fn crate_name(&self) -> Option<&'static str> {
                #crate_name_impl
            }

            fn repo_url(&self) -> Option<&'static str> {
                #repo_url_impl
            }

            fn docs_url(&self) -> Option<&'static str> {
                #docs_url_impl
            }

            fn git_commit(&self) -> Option<&'static str> {
                #git_commit_impl
            }
        }
    }
}
