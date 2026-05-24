use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parse;
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;
use syn::{Token, parse_quote_spanned};

enum ObserveKind {
    Closure(#[expect(dead_code)] Token![|], #[expect(dead_code)] Token![|]),
    Arm(#[expect(dead_code)] Token![=>]),
}

pub struct ObserveInput {
    kind: ObserveKind,
    pat: syn::Pat,
    body: syn::Expr,
}

impl Parse for ObserveInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let or1 = input.parse::<Token![|]>().ok();
        let mut pat = syn::Pat::parse_single(input)?;
        if let Ok(colon) = input.parse::<Token![:]>() {
            let ty: syn::Type = input.parse()?;
            pat = syn::Pat::Type(syn::PatType {
                attrs: vec![],
                pat: Box::new(pat),
                colon_token: colon,
                ty: Box::new(ty),
            });
        }
        let kind = if let Some(or1) = or1 {
            let or2 = input.parse::<Token![|]>()?;
            ObserveKind::Closure(or1, or2)
        } else {
            let fat_arrow = input.parse::<Token![=>]>()?;
            ObserveKind::Arm(fat_arrow)
        };
        let body = input.parse()?;
        Ok(Self { kind, pat, body })
    }
}

fn build_output(pat: &syn::Pat, inits: &mut Vec<TokenStream>) -> Result<TokenStream, TokenStream> {
    match pat {
        syn::Pat::Ident(syn::PatIdent { ident, .. }) => {
            inits.push(quote! { let mut #ident = #ident.__observe(); });
            Ok(quote! {
                match ::muon::observe::SerializeObserverExt::flush(&mut #ident) {
                    Ok(mutation) => mutation,
                    Err(error) => break 'ob Err(error),
                }
            })
        }
        syn::Pat::Tuple(syn::PatTuple { elems, .. }) => {
            let mut outputs = vec![];
            let mut errors = TokenStream::new();
            for pat in elems {
                match build_output(pat, inits) {
                    Ok(output) => outputs.push(output),
                    Err(error) => errors.extend(error),
                }
            }
            if errors.is_empty() {
                Ok(quote! { (#(#outputs),*,) })
            } else {
                Err(errors)
            }
        }
        syn::Pat::Type(syn::PatType { pat, .. }) => build_output(pat, inits),
        syn::Pat::Wild(_) => Ok(quote! {
            match ::muon::Adapter::from_mutations(::muon::Mutations::new()) {
                Ok(value) => value,
                Err(error) => break 'ob Err(error),
            }
        }),
        _ => Err(syn::Error::new(pat.span(), "only ident or tuple patterns are supported").to_compile_error()),
    }
}

pub fn observe(mut input: ObserveInput) -> TokenStream {
    let mut inits = vec![];
    let pat = &input.pat;
    let output = match build_output(pat, &mut inits) {
        Ok(output) => quote! { Ok(#output) },
        Err(errors) => return errors,
    };

    let body = &mut input.body;
    TransformQuasiObserver.visit_expr_mut(body);

    let body = quote! {
        'ob: {
            #[allow(unused_imports)]
            use ::muon::helper::QuasiObserver;
            use ::muon::observe::ObserveExt;
            #(#inits)*
            #[allow(clippy::needless_borrow)]
            #body;
            #output
        }
    };

    match input.kind {
        ObserveKind::Closure(_, _) => quote! { |#pat| #body },
        ObserveKind::Arm(_) => body,
    }
}

struct TransformQuasiObserver;

impl VisitMut for TransformQuasiObserver {
    fn visit_expr_assign_mut(&mut self, expr_assign: &mut syn::ExprAssign) {
        syn::visit_mut::visit_expr_assign_mut(self, expr_assign);
        let left = &expr_assign.left;
        let span = left.span();
        expr_assign.left = parse_quote_spanned! { span =>
            *(&mut #left).tracked_mut()
        };
    }

    fn visit_expr_binary_mut(&mut self, expr_binary: &mut syn::ExprBinary) {
        syn::visit_mut::visit_expr_binary_mut(self, expr_binary);
        match &expr_binary.op {
            syn::BinOp::Eq(_)
            | syn::BinOp::Ne(_)
            | syn::BinOp::Le(_)
            | syn::BinOp::Lt(_)
            | syn::BinOp::Ge(_)
            | syn::BinOp::Gt(_) => {
                let left = &expr_binary.left;
                let span = left.span();
                expr_binary.left = parse_quote_spanned! { span =>
                    *(&#left).untracked_ref()
                };
                let right = &expr_binary.right;
                let span = right.span();
                expr_binary.right = parse_quote_spanned! { span =>
                    *(&#right).untracked_ref()
                };
            }
            _ => {}
        }
    }
}
