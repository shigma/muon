use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::parse_quote;
use syn::spanned::Spanned;

use crate::derive::GenericsDetector;

pub fn derive_snapshot(input: &syn::DeriveInput) -> TokenStream {
    let mut snapshot = input.clone();
    let mut input_name = input.ident.to_string();
    if input_name.starts_with("r#") {
        input_name = input_name[2..].to_string();
    }
    let snap_ident = syn::Ident::new(&format!("{input_name}Snapshot"), input.ident.span());
    snapshot.attrs = vec![];
    snapshot.ident = snap_ident.clone();

    let where_predicates = &mut snapshot.generics.make_where_clause().predicates;
    match &mut snapshot.data {
        syn::Data::Struct(data_struct) => match &mut data_struct.fields {
            syn::Fields::Named(fields) => {
                for field in &mut fields.named {
                    field.attrs = vec![];
                    if GenericsDetector::detect(&field.ty, &input.generics) {
                        let field_ty = &field.ty;
                        where_predicates.push(parse_quote! {
                            #field_ty: ::muon::general::Snapshot
                        });
                        field.ty = parse_quote! {
                            <#field_ty as ::muon::general::Snapshot>::Snapshot
                        };
                    }
                }
            }
            syn::Fields::Unnamed(fields) => {
                for field in &mut fields.unnamed {
                    field.attrs = vec![];
                    if GenericsDetector::detect(&field.ty, &input.generics) {
                        let field_ty = &field.ty;
                        where_predicates.push(parse_quote! {
                            #field_ty: ::muon::general::Snapshot
                        });
                        field.ty = parse_quote! {
                            <#field_ty as ::muon::general::Snapshot>::Snapshot
                        };
                    }
                }
            }
            syn::Fields::Unit => {}
        },
        syn::Data::Enum(data_enum) => {
            for variant in &mut data_enum.variants {
                variant.attrs = vec![];
                match &mut variant.fields {
                    syn::Fields::Named(fields) => {
                        for field in &mut fields.named {
                            field.attrs = vec![];
                            if GenericsDetector::detect(&field.ty, &input.generics) {
                                let field_ty = &field.ty;
                                where_predicates.push(parse_quote! {
                                    #field_ty: ::muon::general::Snapshot
                                });
                                field.ty = parse_quote! {
                                    <#field_ty as ::muon::general::Snapshot>::Snapshot
                                };
                            }
                        }
                    }
                    syn::Fields::Unnamed(fields) => {
                        for field in &mut fields.unnamed {
                            field.attrs = vec![];
                            if GenericsDetector::detect(&field.ty, &input.generics) {
                                let field_ty = &field.ty;
                                where_predicates.push(parse_quote! {
                                    #field_ty: ::muon::general::Snapshot
                                });
                                field.ty = parse_quote! {
                                    <#field_ty as ::muon::general::Snapshot>::Snapshot
                                };
                            }
                        }
                    }
                    syn::Fields::Unit => {}
                }
            }
        }
        syn::Data::Union(_data_union) => {
            return syn::Error::new(input.span(), "PartialEq cannot be derived for unions").to_compile_error();
        }
    }

    let (to_snapshot, eq_snapshot) = match &input.data {
        syn::Data::Struct(data_struct) => match &data_struct.fields {
            syn::Fields::Named(fields) => {
                let field_values = fields.named.iter().map(|field| {
                    let ident = field.ident.as_ref().unwrap();
                    let span = field.span();
                    quote_spanned! { span => #ident: ::muon::general::Snapshot::to_snapshot(&self.#ident) }
                });
                let cmp_values = fields.named.iter().map(|field| {
                    let ident = field.ident.as_ref().unwrap();
                    let span = field.span();
                    quote_spanned! { span => ::muon::general::Snapshot::eq_snapshot(&self.#ident, &snapshot.#ident) }
                });
                (
                    quote! { #snap_ident { #(#field_values),* } },
                    quote! { #(#cmp_values) &&* },
                )
            }
            syn::Fields::Unnamed(fields) => {
                let field_values = fields.unnamed.iter().enumerate().map(|(i, field)| {
                    let index = syn::Index::from(i);
                    let span = field.span();
                    quote_spanned! { span => ::muon::general::Snapshot::to_snapshot(&self.#index) }
                });
                let cmp_values = fields.unnamed.iter().enumerate().map(|(i, field)| {
                    let index = syn::Index::from(i);
                    let span = field.span();
                    quote_spanned! { span => ::muon::general::Snapshot::eq_snapshot(&self.#index, &snapshot.#index) }
                });
                (
                    quote! { #snap_ident ( #(#field_values),* ) },
                    quote! { #(#cmp_values) &&* },
                )
            }
            syn::Fields::Unit => (quote! { #snap_ident }, quote! { true }),
        },
        syn::Data::Enum(data_enum) => {
            let (to_snapshot, eq_snapshot): (Vec<_>, Vec<_>) = data_enum.variants.iter().map(|variant| {
                let variant_ident = &variant.ident;
                match &variant.fields {
                    syn::Fields::Named(fields) => {
                        let field_idents = fields
                            .named
                            .iter()
                            .map(|f| f.ident.as_ref().unwrap())
                            .collect::<Vec<_>>();
                        let field_values = fields.named.iter().map(|field| {
                            let ident = field.ident.as_ref().unwrap();
                            let span = field.span();
                            quote_spanned! { span => #ident: ::muon::general::Snapshot::to_snapshot(#ident) }
                        });
                        let self_idents = fields
                            .named
                            .iter()
                            .enumerate()
                            .map(|(i, f)| syn::Ident::new(&format!("__self_{}", i), f.span()))
                            .collect::<Vec<_>>();
                        let snap_idents = fields
                            .named
                            .iter()
                            .enumerate()
                            .map(|(i, f)| syn::Ident::new(&format!("__snap_{}", i), f.span()))
                            .collect::<Vec<_>>();
                        let cmp_values = fields.named.iter().enumerate().map(|(i, field)| {
                            let span = field.span();
                            let self_ident = &self_idents[i];
                            let snap_ident = &snap_idents[i];
                            quote_spanned! { span => ::muon::general::Snapshot::eq_snapshot(&#self_ident, &#snap_ident) }
                        });
                        let cmp_expr = match fields.named.is_empty() {
                            true => quote! { true },
                            false => quote! { #(#cmp_values) &&* },
                        };
                        (
                            quote! {
                                Self::#variant_ident { #(#field_idents),* } => #snap_ident::#variant_ident { #(#field_values),* }
                            },
                            quote! {
                                (
                                    Self::#variant_ident { #(#field_idents: #self_idents),* },
                                    #snap_ident::#variant_ident { #(#field_idents: #snap_idents),* },
                                ) => #cmp_expr
                            },
                        )
                    }
                    syn::Fields::Unnamed(fields) => {
                        let field_idents = fields
                            .unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, field)| syn::Ident::new(&format!("__self_{}", i), field.span()))
                            .collect::<Vec<_>>();
                        let field_values = field_idents.iter().map(|ident| {
                            let span = ident.span();
                            quote_spanned! { span => ::muon::general::Snapshot::to_snapshot(#ident) }
                        });
                        let self_idents = fields
                            .unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, field)| syn::Ident::new(&format!("__self_{}", i), field.span()))
                            .collect::<Vec<_>>();
                        let snap_idents = fields
                            .unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, field)| syn::Ident::new(&format!("__snap_{}", i), field.span()))
                            .collect::<Vec<_>>();
                        let cmp_values = fields.unnamed.iter().enumerate().map(|(i, field)| {
                            let span = field.span();
                            let self_ident = &self_idents[i];
                            let snap_ident = &snap_idents[i];
                            quote_spanned! { span => ::muon::general::Snapshot::eq_snapshot(&#self_ident, &#snap_ident) }
                        });
                        let cmp_expr = match fields.unnamed.is_empty() {
                            true => quote! { true },
                            false => quote! { #(#cmp_values) &&* },
                        };
                        (
                            quote! {
                                Self::#variant_ident( #(#field_idents),* ) => #snap_ident::#variant_ident( #(#field_values),* )
                            },
                            quote! {
                                (
                                    Self::#variant_ident( #(#self_idents),* ),
                                    #snap_ident::#variant_ident( #(#snap_idents),* ),
                                ) => #cmp_expr
                            },
                        )
                    }
                    syn::Fields::Unit => (
                        quote! { Self::#variant_ident => #snap_ident::#variant_ident },
                        quote! { (Self::#variant_ident, #snap_ident::#variant_ident) => true },
                    ),
                }
            }).unzip();
            (
                quote! {
                    match self {
                        #(#to_snapshot,)*
                    }
                },
                quote! {
                    match (self, snapshot) {
                        #(#eq_snapshot,)*
                        _ => false,
                    }
                },
            )
        }
        syn::Data::Union(_data_union) => unreachable!(),
    };

    let input_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = snapshot.generics.split_for_impl();
    quote! {
        const _: () = {
            #snapshot

            #[automatically_derived]
            impl #impl_generics ::muon::general::Snapshot for #input_ident #ty_generics #where_clause {
                type Snapshot = #snap_ident #ty_generics;
                fn to_snapshot(&self) -> Self::Snapshot {
                    #to_snapshot
                }
                #[allow(clippy::match_like_matches_macro)]
                fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
                    #eq_snapshot
                }
            }
        };
    }
}

pub fn derive_noop_snapshot(input: &syn::DeriveInput) -> TokenStream {
    let input_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    quote! {
        #[automatically_derived]
        impl #impl_generics ::muon::general::Snapshot for #input_ident #ty_generics #where_clause {
            type Snapshot = ();
            fn to_snapshot(&self) {}
            fn eq_snapshot(&self, snapshot: &()) -> bool {
                true
            }
        }
    }
}

pub fn derive_default(_input: &syn::DeriveInput) -> TokenStream {
    quote! {}
}
