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
                            #field_ty: ::muon::general::SerializeSnapshot
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
                            #field_ty: ::muon::general::SerializeSnapshot
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
                                    #field_ty: ::muon::general::SerializeSnapshot
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
                                    #field_ty: ::muon::general::SerializeSnapshot
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

    let (to_snapshot, flush_body) = match &input.data {
        syn::Data::Struct(data_struct) => match &data_struct.fields {
            syn::Fields::Named(fields) if fields.named.is_empty() => {
                (quote! { #snap_ident {} }, quote! { ::muon::Mutations::new() })
            }
            syn::Fields::Named(fields) => {
                let field_values = fields.named.iter().map(|field| {
                    let ident = field.ident.as_ref().unwrap();
                    let span = field.span();
                    quote_spanned! { span => #ident: ::muon::general::Snapshot::to_snapshot(&self.#ident) }
                });
                let flush_lets = fields.named.iter().map(|field| {
                    let ident = field.ident.as_ref().unwrap();
                    let span = field.span();
                    let name = ident.to_string();
                    quote_spanned! { span =>
                        let #ident = ::muon::general::SerializeSnapshot::flush(&self.#ident, snapshot.#ident)
                            .with_prefix(#name);
                    }
                });
                let field_idents: Vec<_> = fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
                (
                    quote! { #snap_ident { #(#field_values),* } },
                    quote! {
                        #(#flush_lets)*
                        if #(#field_idents.is_replace())&&* {
                            ::muon::Mutations::replace(self)
                        } else {
                            let mut mutations = ::muon::Mutations::new();
                            #(mutations.extend(#field_idents);)*
                            mutations
                        }
                    },
                )
            }
            syn::Fields::Unnamed(fields) if fields.unnamed.is_empty() => {
                (quote! { #snap_ident () }, quote! { ::muon::Mutations::new() })
            }
            syn::Fields::Unnamed(fields) => {
                let field_values = fields.unnamed.iter().enumerate().map(|(i, field)| {
                    let index = syn::Index::from(i);
                    let span = field.span();
                    quote_spanned! { span => ::muon::general::Snapshot::to_snapshot(&self.#index) }
                });
                let temp_idents: Vec<_> = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(i, f)| syn::Ident::new(&format!("__field_{}", i), f.span()))
                    .collect();
                let flush_lets = fields.unnamed.iter().enumerate().map(|(i, field)| {
                    let index = syn::Index::from(i);
                    let span = field.span();
                    let temp = &temp_idents[i];
                    quote_spanned! { span =>
                        let #temp = ::muon::general::SerializeSnapshot::flush(&self.#index, snapshot.#index)
                            .with_prefix(#i as u64);
                    }
                });
                (
                    quote! { #snap_ident ( #(#field_values),* ) },
                    quote! {
                        #(#flush_lets)*
                        if #(#temp_idents.is_replace())&&* {
                            ::muon::Mutations::replace(self)
                        } else {
                            let mut mutations = ::muon::Mutations::new();
                            #(mutations.extend(#temp_idents);)*
                            mutations
                        }
                    },
                )
            }
            syn::Fields::Unit => (quote! { #snap_ident }, quote! { ::muon::Mutations::new() }),
        },
        syn::Data::Enum(data_enum) => {
            let (to_snapshot_arms, flush_arms): (Vec<_>, Vec<_>) = data_enum.variants.iter().map(|variant| {
                let variant_ident = &variant.ident;
                match &variant.fields {
                    syn::Fields::Named(fields) if fields.named.is_empty() => (
                        quote! {
                            Self::#variant_ident {} => #snap_ident::#variant_ident {}
                        },
                        quote! {
                            (Self::#variant_ident {}, #snap_ident::#variant_ident {}) => ::muon::Mutations::new()
                        },
                    ),
                    syn::Fields::Named(fields) => {
                        let field_idents: Vec<_> = fields
                            .named
                            .iter()
                            .map(|f| f.ident.as_ref().unwrap())
                            .collect();
                        let field_values = fields.named.iter().map(|field| {
                            let ident = field.ident.as_ref().unwrap();
                            let span = field.span();
                            quote_spanned! { span => #ident: ::muon::general::Snapshot::to_snapshot(#ident) }
                        });
                        let self_idents: Vec<_> = fields
                            .named
                            .iter()
                            .enumerate()
                            .map(|(i, f)| syn::Ident::new(&format!("__self_{}", i), f.span()))
                            .collect();
                        let snap_idents: Vec<_> = fields
                            .named
                            .iter()
                            .enumerate()
                            .map(|(i, f)| syn::Ident::new(&format!("__snap_{}", i), f.span()))
                            .collect();
                        let flush_lets = fields.named.iter().enumerate().map(|(i, field)| {
                            let ident = field.ident.as_ref().unwrap();
                            let span = field.span();
                            let self_ident = &self_idents[i];
                            let snap_ident = &snap_idents[i];
                            let name = ident.to_string();
                            quote_spanned! { span =>
                                let #ident = ::muon::general::SerializeSnapshot::flush(#self_ident, #snap_ident)
                                    .with_prefix(#name);
                            }
                        });
                        (
                            quote! {
                                Self::#variant_ident { #(#field_idents),* } => #snap_ident::#variant_ident { #(#field_values),* }
                            },
                            quote! {
                                (
                                    Self::#variant_ident { #(#field_idents: #self_idents),* },
                                    #snap_ident::#variant_ident { #(#field_idents: #snap_idents),* },
                                ) => {
                                    #(#flush_lets)*
                                    if #(#field_idents.is_replace())&&* {
                                        ::muon::Mutations::replace(self)
                                    } else {
                                        let mut mutations = ::muon::Mutations::new();
                                        #(mutations.extend(#field_idents);)*
                                        mutations
                                    }
                                }
                            },
                        )
                    }
                    syn::Fields::Unnamed(fields) if fields.unnamed.is_empty() => (
                        quote! {
                            Self::#variant_ident() => #snap_ident::#variant_ident()
                        },
                        quote! {
                            (Self::#variant_ident(), #snap_ident::#variant_ident()) => ::muon::Mutations::new()
                        },
                    ),
                    syn::Fields::Unnamed(fields) => {
                        let self_idents: Vec<_> = fields
                            .unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, field)| syn::Ident::new(&format!("__self_{}", i), field.span()))
                            .collect();
                        let field_values = self_idents.iter().map(|ident| {
                            let span = ident.span();
                            quote_spanned! { span => ::muon::general::Snapshot::to_snapshot(#ident) }
                        });
                        let snap_idents: Vec<_> = fields
                            .unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, field)| syn::Ident::new(&format!("__snap_{}", i), field.span()))
                            .collect();
                        let temp_idents: Vec<_> = fields
                            .unnamed
                            .iter()
                            .enumerate()
                            .map(|(i, f)| syn::Ident::new(&format!("__field_{}", i), f.span()))
                            .collect();
                        let flush_lets = fields.unnamed.iter().enumerate().map(|(i, field)| {
                            let span = field.span();
                            let self_ident = &self_idents[i];
                            let snap_ident = &snap_idents[i];
                            let temp = &temp_idents[i];
                            quote_spanned! { span =>
                                let #temp = ::muon::general::SerializeSnapshot::flush(#self_ident, #snap_ident)
                                    .with_prefix(#i as u64);
                            }
                        });
                        (
                            quote! {
                                Self::#variant_ident( #(#self_idents),* ) => #snap_ident::#variant_ident( #(#field_values),* )
                            },
                            quote! {
                                (
                                    Self::#variant_ident( #(#self_idents),* ),
                                    #snap_ident::#variant_ident( #(#snap_idents),* ),
                                ) => {
                                    #(#flush_lets)*
                                    if #(#temp_idents.is_replace())&&* {
                                        ::muon::Mutations::replace(self)
                                    } else {
                                        let mut mutations = ::muon::Mutations::new();
                                        #(mutations.extend(#temp_idents);)*
                                        mutations
                                    }
                                }
                            },
                        )
                    }
                    syn::Fields::Unit => (
                        quote! { Self::#variant_ident => #snap_ident::#variant_ident },
                        quote! { (Self::#variant_ident, #snap_ident::#variant_ident) => ::muon::Mutations::new() },
                    ),
                }
            }).unzip();
            (
                quote! {
                    match self {
                        #(#to_snapshot_arms,)*
                    }
                },
                quote! {
                    match (self, snapshot) {
                        #(#flush_arms,)*
                        _ => ::muon::Mutations::replace(self),
                    }
                },
            )
        }
        syn::Data::Union(_data_union) => unreachable!(),
    };

    let input_ident = &input.ident;
    let mut serialize_generics = snapshot.generics.clone();
    serialize_generics
        .make_where_clause()
        .predicates
        .push(parse_quote! { Self: ::serde::Serialize });
    let (impl_generics, ty_generics, where_clause) = snapshot.generics.split_for_impl();
    let (_, _, serialize_where_clause) = serialize_generics.split_for_impl();
    quote! {
        const _: () = {
            #snapshot

            #[automatically_derived]
            impl #impl_generics ::muon::general::Snapshot for #input_ident #ty_generics #where_clause {
                type Snapshot = #snap_ident #ty_generics;
                fn to_snapshot(&self) -> Self::Snapshot {
                    #to_snapshot
                }
            }

            #[automatically_derived]
            impl #impl_generics ::muon::general::SerializeSnapshot for #input_ident #ty_generics #serialize_where_clause {
                fn flush(&self, snapshot: Self::Snapshot) -> ::muon::Mutations {
                    #flush_body
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
        }

        #[automatically_derived]
        impl #impl_generics ::muon::general::SerializeSnapshot for #input_ident #ty_generics #where_clause {
            fn flush(&self, _snapshot: ()) -> ::muon::Mutations {
                ::muon::Mutations::new()
            }
        }
    }
}

pub fn derive_default(_input: &syn::DeriveInput) -> TokenStream {
    quote! {}
}
