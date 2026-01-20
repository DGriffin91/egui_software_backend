//! # constify
//!
//! Example usage:
//! ```rust
//! use constify::constify;
//! #[constify]
//! fn foo(
//!     #[constify] a: bool,
//!     #[constify] b: bool,
//!     c: bool
//! ) -> u32 {
//!     let mut sum = 0;
//!     if a {
//!         sum += 1;
//!     }
//!     if b {
//!         sum += 10;
//!     }
//!     if c {
//!         sum += 100;
//!     }
//!     sum
//! }
//! ```
//!
//! Expansion:
//! ```rust
//! #[inline(always)]
//! fn foo(a: bool, b: bool, c: bool) -> u32 {
//!     fn foo<const a: bool, const b: bool>(c: bool) -> u32 {
//!         let mut sum = 0;
//!         if a {
//!             sum += 1;
//!         }
//!         if b {
//!             sum += 10;
//!         }
//!         if c {
//!             sum += 100;
//!         }
//!         sum
//!     }
//!     match (a, b) {
//!         (false, false) => foo::<false, false>(c),
//!         (true, false) => foo::<true, false>(c),
//!         (false, true) => foo::<false, true>(c),
//!         (true, true) => foo::<true, true>(c),
//!     }
//! }
//! ```
//!
//! Inspired by <https://github.com/TennyZhuang/const-currying-rs>

use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{
    Block, ConstParam, FnArg, GenericParam, ItemFn, Pat, PatIdent, Result, Token, Type,
    parse_macro_input, parse_quote, spanned::Spanned,
};

#[proc_macro_attribute]
pub fn constify(_attr: TokenStream1, item: TokenStream1) -> TokenStream1 {
    let input = parse_macro_input!(item as ItemFn);
    match inner(input) {
        Ok(output) => output.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn remove_attr(arg: &FnArg) -> FnArg {
    match arg.clone() {
        FnArg::Typed(mut typed) => {
            typed.attrs.clear();
            FnArg::Typed(typed)
        }
        r @ FnArg::Receiver(_) => r,
    }
}

const CONSTIFY: &str = "constify";
fn inner(item: ItemFn) -> Result<TokenStream> {
    let ItemFn {
        sig, attrs, block, ..
    } = &item;

    // collect constifyed bool targets
    let mut constify_args = Vec::new();
    let mut non_constify_args = Vec::new();
    for input in sig.inputs.iter() {
        if let FnArg::Typed(typed) = input {
            if typed.attrs.iter().any(|a| a.path().is_ident(CONSTIFY)) {
                let Pat::Ident(PatIdent {
                    ident: arg_name, ..
                }) = &*typed.pat
                else {
                    return Err(syn::Error::new(
                        typed.pat.span(),
                        "Only simple identifiers are supported for constifyed args",
                    ));
                };
                if !matches!(&*typed.ty, Type::Path(tp) if tp.path.is_ident("bool")) {
                    return Err(syn::Error::new(
                        typed.ty.span(),
                        "#[constify] only supports `bool` parameters",
                    ));
                }
                constify_args.push((arg_name.clone(), typed.clone()));
                continue;
            }
        }
        non_constify_args.push(remove_attr(input));
    }

    // build specialized fn (add const<bool> generics and strip constifyed args)
    if constify_args.is_empty() {
        return Ok(quote! { #item });
    }

    let mut new_sig = sig.clone();

    // convert Vec<FnArg> non_constify_args into Punctuated<FnArg, Comma>
    new_sig.inputs = non_constify_args.into_iter().collect();

    new_sig
        .generics
        .params
        .extend(constify_args.iter().map(|(arg_name, input)| {
            GenericParam::Const(ConstParam {
                attrs: vec![],
                const_token: Token![const](arg_name.span()),
                ident: arg_name.clone(),
                colon_token: input.colon_token,
                ty: parse_quote!(bool),
                default: None,
                eq_token: None,
            })
        }));

    let mut new_attrs = attrs.clone();
    new_attrs.push(parse_quote!(#[allow(warnings)]));

    let specialized_fn = ItemFn {
        sig: new_sig,
        attrs: new_attrs,
        block: block.clone(),
        ..item.clone()
    };

    let non_constifyed_arg_ts = specialized_fn.sig.inputs.iter()
        .map(| input| match input {
            FnArg::Receiver(_) => quote! { self },
            FnArg::Typed(typed) => match &*typed.pat {
                Pat::Ident(p) => {
                    let name = &p.ident;
                    quote! { #name }
                }
                _ => quote_spanned! { typed.pat.span()=> compile_error!("Only simple identifiers are supported for non-constify args"); },
            }
        })
        .collect::<Vec<_>>();

    // build branches for all 2^N bool combinations
    let n = constify_args.len();
    let total = 1usize << n;
    let mut branches = Vec::with_capacity(total);

    for mask in 0..total {
        // bool literals in target order
        let mut match_args = Vec::with_capacity(n);
        let mut added_const_args = Vec::with_capacity(n);

        for i in 0..n {
            let b = ((mask >> i) & 1) == 1;
            let lit = if b { quote!(true) } else { quote!(false) };
            match_args.push(lit.clone());
            added_const_args.push(lit);
        }

        // <type_generics..., const_generics..., added_bool_consts...>
        let all_generic_args = sig
            .generics
            .params
            .iter()
            .filter_map(|p| {
                let id = match p {
                    GenericParam::Type(t) => &t.ident,
                    GenericParam::Const(c) => &c.ident,
                    _ => return None,
                };
                Some(quote! { #id })
            })
            .chain(added_const_args.into_iter());
        let generic_appl = quote! { ::<#(#all_generic_args),*> };

        let fn_ident = &sig.ident;
        let call = if non_constifyed_arg_ts.is_empty() {
            quote! { #fn_ident #generic_appl () }
        } else {
            quote! { #fn_ident #generic_appl (#(#non_constifyed_arg_ts),*,) }
        };

        branches.push(quote! { (#(#match_args),*) => { #call } });
    }

    // outer constifyer with original signature (minus arg attrs)
    let constify_fn: ItemFn = {
        let mut new_sig = sig.clone();
        new_sig.inputs = sig.inputs.iter().map(remove_attr).collect();

        let mut new_attrs = attrs.clone();
        new_attrs.push(parse_quote!(#[inline(always)]));

        let all_target_names: Vec<_> = constify_args.iter().map(|(name, _)| name).collect();

        let body: Block = parse_quote! {{
            #specialized_fn
            match (#(#all_target_names),*) {
                #(#branches),*
            }
        }};

        ItemFn {
            sig: new_sig,
            attrs: new_attrs,
            block: Box::new(body),
            vis: item.vis.clone(),
        }
    };

    Ok(quote! { #constify_fn })
}
