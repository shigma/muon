use std::mem::take;

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{parse_quote, parse_quote_spanned};

use crate::derive::meta::{AttributeKind, DeriveKind, GeneralImpl, ObserveMeta};
use crate::derive::{FMT_TRAITS, GenericsDetector, GenericsVisitor};

pub fn derive_observe_for_enum(
    input: &syn::DeriveInput,
    variants: &Punctuated<syn::Variant, syn::Token![,]>,
    input_meta: &ObserveMeta,
) -> TokenStream {
    let input_ident = &input.ident;
    // let ob_name = format!("{}Observer", input_ident);
    let ob_ident = format_ident!("{}Observer", input_ident);
    let ob_initial_ident = format_ident!("{}ObserverInitial", input_ident);
    let ob_variant_ident = format_ident!("{}ObserverVariant", input_ident);
    let input_vis = &input.vis;

    let mut generics_visitor = GenericsVisitor::default();
    generics_visitor.visit_derive_input(input);
    let head = generics_visitor.allocate_ty(parse_quote!(S));
    let depth = generics_visitor.allocate_ty(parse_quote!(N));
    let ob_lt = generics_visitor.allocate_lt(parse_quote!('ob));

    let mut ob_initial_variants = quote! {};
    let mut ob_variant_variants = quote! {};
    let mut initial_observe_arms = quote! {};
    let mut initial_flush_pats = quote! {};
    let mut variant_observe_arms = quote! {};
    let mut variant_relocate_arms = quote! {};
    let mut variant_flush_arms = quote! {};
    let mut variant_flat_flush_arms = quote! {};

    let mut errors = quote! {};
    let mut field_tys = vec![];
    let mut ob_field_tys = vec![];
    let mut skipped_tys = vec![];
    let mut general_predicates = vec![];
    let mut has_variant = false;
    let mut has_initial = false;
    for variant in variants {
        let variant_ident = &variant.ident;
        let variant_name = variant.ident.to_string();
        if variant.fields.is_empty() {
            has_initial = true;
            let mut variant = variant.clone();
            take(&mut variant.attrs);
            ob_initial_variants.extend(quote! {
                #variant_ident,
            });
            initial_observe_arms.extend(quote! {
                #input_ident::#variant => #ob_initial_ident::#variant_ident,
            });
            initial_flush_pats.extend(quote! {
                | (#ob_initial_ident::#variant_ident, #input_ident::#variant)
            });
            continue;
        }

        has_variant = true;
        let variant_meta =
            ObserveMeta::parse_attrs(&variant.attrs, &mut errors, AttributeKind::Variant, DeriveKind::Enum);
        let tag_segment = if variant_meta.serde.untagged {
            None
        } else if let Some(rename) = &variant_meta.serde.rename {
            Some(quote! { #rename })
        } else if let Some(expr) = &input_meta.serde.content {
            Some(quote! { #expr })
        } else if input_meta.serde.untagged || input_meta.serde.tag.is_some() {
            None
        } else {
            let segment = input_meta.serde.rename_all.apply(&variant_name);
            Some(quote! { #segment })
        };

        let if_named: Vec<TokenStream> = match &variant.fields {
            syn::Fields::Named(_) => vec![quote! {}],
            _ => vec![],
        };

        let mut idents = vec![];
        let mut ob_idents = vec![];
        let mut value_idents = vec![];
        let mut flush_idents = vec![];
        let mut variant_fields = quote! {};
        let mut observe_fields = quote! {};
        let mut relocate_stmts = quote! {};
        let mut mutation_idents = vec![];
        let mut flush_field_stmts = quote! {};
        let mut flush_mutation_stmts = quote! {};
        let mut flush_capacity = vec![];
        let mut has_skipped = false;

        let field_count = variant.fields.len();
        for (index, field) in variant.fields.iter().enumerate() {
            let field_meta =
                ObserveMeta::parse_attrs(&field.attrs, &mut errors, AttributeKind::Field, DeriveKind::Enum);
            let mut field_cloned = field.clone();
            field_cloned.attrs = vec![];
            let field_span = field_cloned.span();
            let field_trivial = !GenericsDetector::detect(&field.ty, &input.generics);
            let field_ty = &field.ty;
            let field_ident = &field.ident;
            let ob_ident = syn::Ident::new(&format!("u{}", index), field_span);
            let value_ident = syn::Ident::new(&format!("v{}", index), field_span);
            if let Some(field_ident) = field_ident {
                idents.push(quote! { #field_ident });
            }
            ob_idents.push(quote! { #ob_ident });
            value_idents.push(quote! { #value_ident });
            let observe_ident = if let Some(field_ident) = field_ident {
                field_ident
            } else {
                &value_ident
            };
            let flush_ident = if let Some(field_ident) = field_ident {
                field_ident
            } else {
                &ob_ident
            };

            if field_meta.skip || field_meta.serde.skip || field_meta.serde.skip_serializing {
                has_skipped = true;
                if !field_trivial {
                    skipped_tys.push(quote! { #field_ty });
                }
                variant_fields.extend(quote! {
                    #(#if_named #field_ident:)* ::muon::helper::Pointer<#field_ty>,
                });
                observe_fields.extend(quote_spanned! { field_span =>
                    #(#if_named #field_ident:)* ::muon::helper::Pointer::new_unchecked(
                        __ptr.with_addr(#observe_ident as *const _ as usize).cast(),
                    ),
                });
                relocate_stmts.extend(quote_spanned! { field_span =>
                    ::muon::helper::Pointer::set(#ob_ident, #value_ident);
                });
                if field_ident.is_none() {
                    flush_idents.push(quote! { _ });
                }
                continue;
            }

            flush_idents.push(quote! { #flush_ident });
            let ob_field_ty: syn::Type = match &field_meta.general_impl {
                None => parse_quote_spanned! { field_span =>
                    ::muon::observe::DefaultObserver<#ob_lt, #field_ty>
                },
                Some(GeneralImpl { ob_ident, .. }) => parse_quote_spanned! { field_span =>
                    ::muon::general::#ob_ident<#ob_lt, #field_ty, #field_ty>
                },
            };
            if !field_trivial {
                if let Some(GeneralImpl { bounds, .. }) = &field_meta.general_impl {
                    skipped_tys.push(quote! { #field_ty });
                    if !bounds.is_empty() {
                        general_predicates.push(quote! { #field_ty: #bounds });
                    }
                } else {
                    field_tys.push(quote! { #field_ty });
                }
                ob_field_tys.push(quote! { #ob_field_ty });
            }
            variant_fields.extend(quote! {
                #(#if_named #field_ident:)* #ob_field_ty,
            });
            observe_fields.extend(quote_spanned! { field_span =>
                #(#if_named #field_ident:)* ::muon::observe::Observer::observe(
                    __ptr.with_addr(#observe_ident as *const _ as usize).cast(),
                ),
            });
            relocate_stmts.extend(quote_spanned! { field_span =>
                ::muon::observe::Observer::relocate(
                    #ob_ident,
                    __ptr.with_addr(#value_ident as *const _ as usize).cast(),
                );
            });

            let mutation_ident;
            let default_segment;
            if let Some(field_ident) = &field_ident {
                let mut field_name = field_ident.to_string();
                if field_name.starts_with("r#") {
                    field_name = field_name[2..].to_string();
                }
                mutation_ident = syn::Ident::new(&format!("mutations_{field_name}"), field_span);
                let segment = input_meta.serde.rename_all.apply(&field_name);
                default_segment = quote! { #segment };
            } else {
                mutation_ident = syn::Ident::new(&format!("mutations_{index}"), field_span);
                default_segment = quote! { #index };
            }

            if field_meta.serde.flatten {
                flush_field_stmts.extend(quote_spanned! { field_span =>
                    let #mutation_ident = ::muon::observe::SerializeObserver::flat_flush(#flush_ident);
                });
                flush_capacity.push(quote_spanned! { field_span =>
                    #mutation_ident.len()
                });
                if cfg!(feature = "delete")
                    && let Some(path) = field_meta.serde.skip_serializing_if
                {
                    flush_mutation_stmts.extend(quote_spanned! { field_span =>
                        if !#mutation_ident.is_empty() && #path(::muon::helper::QuasiObserver::untracked_ref(#flush_ident)) {
                            mutations.extend(#mutation_ident.into_delete());
                        } else {
                            mutations.extend(#mutation_ident);
                        }
                    });
                } else {
                    flush_mutation_stmts.extend(quote_spanned! { field_span =>
                        mutations.extend(#mutation_ident);
                    });
                }
            } else {
                flush_field_stmts.extend(quote_spanned! { field_span =>
                    let #mutation_ident = ::muon::observe::SerializeObserver::flush(#flush_ident);
                });
                flush_capacity.push(quote_spanned! { field_span =>
                    !#mutation_ident.is_empty() as usize
                });
                let segment = if let Some(rename) = &field_meta.serde.rename {
                    quote! { #rename }
                } else {
                    default_segment
                };
                if cfg!(feature = "delete")
                    && let Some(path) = field_meta.serde.skip_serializing_if
                {
                    flush_mutation_stmts.extend(quote_spanned! { field_span =>
                        if !#mutation_ident.is_empty() && #path(::muon::helper::QuasiObserver::untracked_ref(#flush_ident)) {
                            mutations.insert(#segment, ::muon::Mutations::delete());
                        } else {
                            mutations.insert(#segment, #mutation_ident);
                        }
                    });
                } else {
                    flush_mutation_stmts.extend(quote_spanned! { field_span =>
                        mutations.insert(#segment, #mutation_ident);
                    });
                }
            }
            mutation_idents.push(mutation_ident);
        }

        let mutations_chain = match &tag_segment {
            Some(segment) => quote! { .with_prefix(#segment) },
            None => quote! {},
        };

        let variant_flush_expr = if flush_capacity.is_empty() {
            quote! { ::muon::Mutations::new() }
        } else if matches!(&variant.fields, syn::Fields::Unnamed(_)) && field_count == 1 {
            let flush_ident = &flush_idents[0];
            quote! {
                ::muon::observe::SerializeObserver::flush(#flush_ident) #mutations_chain
            }
        } else {
            quote! {{
                #flush_field_stmts
                if #(#mutation_idents.is_replace())&&* {
                    return ::muon::Mutations::replace(unsafe { &*__ptr });
                }
                let mut mutations = ::muon::Mutations::new().with_capacity(#(#flush_capacity)+*);
                #flush_mutation_stmts
                mutations #mutations_chain
            }}
        };

        let variant_flat_flush_expr = match &variant.fields {
            syn::Fields::Named(_) => {
                if flush_capacity.is_empty() {
                    Some(quote! { ::muon::Mutations::new() })
                } else {
                    Some(quote! {{
                        #flush_field_stmts
                        let mut mutations = ::muon::Mutations::new()
                            .with_capacity(#(#flush_capacity)+*)
                            .with_replace(#(#mutation_idents.is_replace())&&*);
                        #flush_mutation_stmts
                        mutations #mutations_chain
                    }})
                }
            }
            syn::Fields::Unnamed(_) if field_count == 1 => {
                if flush_capacity.is_empty() {
                    Some(quote! { ::muon::Mutations::new() })
                } else {
                    let flush_ident = &flush_idents[0];
                    Some(quote! {
                        ::muon::observe::SerializeObserver::flat_flush(#flush_ident) #mutations_chain
                    })
                }
            }
            _ => None,
        };

        match &variant.fields {
            syn::Fields::Named(_) => {
                if has_skipped {
                    flush_idents.push(quote! { .. });
                }
                ob_variant_variants.extend(quote! {
                    #variant_ident { #variant_fields },
                });
                variant_observe_arms.extend(quote! {
                    #input_ident::#variant_ident { #(#idents,)* } => Self::#variant_ident { #observe_fields },
                });
                variant_relocate_arms.extend(quote! {
                    (Self::#variant_ident { #(#idents: #ob_idents,)* }, #input_ident::#variant_ident { #(#idents: #value_idents,)* }) => { #relocate_stmts }
                });
                variant_flush_arms.extend(quote! {
                    Self::#variant_ident { #(#flush_idents),* } => #variant_flush_expr,
                });
                variant_flat_flush_arms.extend(quote! {
                    Self::#variant_ident { #(#flush_idents),* } => #variant_flat_flush_expr,
                });
            }
            syn::Fields::Unnamed(_) => {
                ob_variant_variants.extend(quote! {
                    #variant_ident(#variant_fields),
                });
                variant_observe_arms.extend(quote! {
                    #input_ident::#variant_ident(#(#value_idents),*) => Self::#variant_ident(#observe_fields),
                });
                variant_relocate_arms.extend(quote! {
                    (Self::#variant_ident(#(#ob_idents),*), #input_ident::#variant_ident(#(#value_idents),*)) => { #relocate_stmts }
                });
                variant_flush_arms.extend(quote! {
                    Self::#variant_ident(#(#flush_idents),*) => #variant_flush_expr,
                });
                if let Some(variant_flat_flush_expr) = variant_flat_flush_expr {
                    variant_flat_flush_arms.extend(quote! {
                        Self::#variant_ident(#(#flush_idents),*) => #variant_flat_flush_expr,
                    });
                }
            }
            syn::Fields::Unit => {
                variant_observe_arms.extend(quote! {
                    #input_ident::#variant_ident => Self::#variant_ident,
                });
                variant_relocate_arms.extend(quote! {
                    (Self::#variant_ident, #input_ident::#variant_ident) => {},
                });
                variant_flush_arms.extend(quote! {
                    Self::#variant_ident => Ok(None),
                });
            }
        }
    }
    if !errors.is_empty() {
        return errors;
    }

    if has_variant {
        ob_initial_variants.extend(quote! { __Unknown, });
        initial_observe_arms.extend(quote! {
            _ => #ob_initial_ident::__Unknown,
        });
    }

    ob_variant_variants.extend(quote! { __Unknown, });
    if has_initial {
        variant_observe_arms.extend(quote! {
            _ => Self::__Unknown,
        });
    }
    variant_relocate_arms.extend(quote! {
        (Self::__Unknown, _) => {},
    });
    variant_flush_arms.extend(quote! {
        Self::__Unknown => ::muon::Mutations::new(),
    });
    variant_flat_flush_arms.extend(quote! {
        _ => panic!("flat_flush can only be called on structs and maps"),
    });

    let ob_flush_prefix_stmt = if has_initial {
        quote! {
            let initial = this.initial;
            this.initial = #ob_initial_ident::new(value);
        }
    } else {
        quote! {}
    };
    let ob_flush_suffix_stmt = if has_initial {
        quote! {
            match (initial, value) {
                #initial_flush_pats => ::muon::Mutations::new(),
                _ => ::muon::Mutations::replace(value),
            }
        }
    } else {
        quote! {
            ::muon::Mutations::replace(this.as_deref())
        }
    };

    let if_has_initial = match has_initial {
        true => vec![quote! {}],
        false => vec![],
    };
    let if_has_variant = match has_variant {
        true => vec![quote! {}],
        false => vec![],
    };

    let inconsistent_state = format!("inconsistent state for {ob_ident}");

    let mut input_generics = input.generics.clone();
    let input_predicates = match take(&mut input_generics.where_clause) {
        Some(where_clause) => where_clause.predicates.into_iter().collect::<Vec<_>>(),
        None => Default::default(),
    };
    let (input_impl_generics, input_type_generics, _) = input_generics.split_for_impl();

    let mut ob_variant_generics = input_generics.clone();
    ob_variant_generics.params.insert(0, parse_quote! { #ob_lt });

    let mut ob_generics = ob_variant_generics.clone();
    ob_generics.params.push(parse_quote! { #head: ?Sized });
    ob_generics.params.push(parse_quote! { #depth = ::muon::helper::Zero });

    let (ob_impl_generics, ob_type_generics, _) = ob_generics.split_for_impl();
    let (ob_variant_impl_generics, ob_variant_type_generics, _) = ob_variant_generics.split_for_impl();

    let input_trivial = input.generics.params.is_empty();
    let input_serialize_predicates = if input_trivial {
        quote! {}
    } else {
        quote! {
            #input_ident #input_type_generics: ::muon::helper::serde::Serialize + 'static,
        }
    };
    let self_serialize_predicates = if input_trivial {
        quote! {}
    } else {
        quote! {
            Self: ::muon::helper::serde::Serialize,
        }
    };

    let derive_idents = &input_meta.derive.0;

    let ob_initial_metas = &input_meta.__initial;
    let ob_initial_impl = quote! {
        #(#[#ob_initial_metas])*
        #[derive(Clone, Copy)]
        #[allow(clippy::enum_variant_names)]
        #input_vis enum #ob_initial_ident {
            #ob_initial_variants
        }

        impl #ob_initial_ident {
            fn new #input_impl_generics(value: &#input_ident #input_type_generics) -> Self
            where
                #(#input_predicates,)*
            {
                match value {
                    #initial_observe_arms
                }
            }
        }
    };

    let ob_variant_metas = &input_meta.__variant;
    let ob_variant_impl = quote! {
        #(#[#ob_variant_metas])*
        #input_vis enum #ob_variant_ident #ob_variant_generics
        where
            #(#input_predicates,)*
            #(#field_tys: ::muon::Observe + #ob_lt,)*
            #(#general_predicates,)*
        {
            #ob_variant_variants
        }

        impl #ob_variant_impl_generics #ob_variant_ident #ob_variant_type_generics
        where
            #(#input_predicates,)*
            #(#field_tys: ::muon::Observe,)*
            #(#general_predicates,)*
        {
            unsafe fn observe(__ptr: *mut #input_ident #input_type_generics) -> Self {
                unsafe { match &*__ptr {
                    #variant_observe_arms
                } }
            }

            unsafe fn relocate(&mut self, __ptr: *mut #input_ident #input_type_generics) {
                unsafe {
                    match (self, &*__ptr) {
                        #variant_relocate_arms
                        _ => panic!(#inconsistent_state),
                    }
                }
            }

            fn flush(&mut self, __ptr: *const #input_ident #input_type_generics) -> ::muon::Mutations
            where
                #input_serialize_predicates
                #(#ob_field_tys: ::muon::observe::SerializeObserver,)*
            {
                match self {
                    #variant_flush_arms
                }
            }

            fn flat_flush(&mut self, __ptr: *const #input_ident #input_type_generics) -> ::muon::Mutations
            where
                #input_serialize_predicates
                #(#ob_field_tys: ::muon::observe::SerializeObserver,)*
            {
                match self {
                    #variant_flat_flush_arms
                }
            }
        }
    };

    let mut output = quote! {
        #(#[::std::prelude::v1::#derive_idents()])*
        #input_vis struct #ob_ident #ob_generics
        where
            #(#input_predicates,)*
            #(#field_tys: ::muon::Observe + #ob_lt,)*
            #(#general_predicates,)*
        {
            ptr: ::muon::helper::Pointer<#head>,
            #(#if_has_variant mutated: bool,)*
            #(#if_has_initial initial: #ob_initial_ident,)*
            #(#if_has_variant variant: #ob_variant_ident #ob_variant_type_generics,)*
            phantom: ::std::marker::PhantomData<&#ob_lt mut #depth>,
        }

        #(#if_has_initial #ob_initial_impl)*

        #(#if_has_variant #ob_variant_impl)*

        #[automatically_derived]
        impl #ob_impl_generics ::std::ops::Deref
        for #ob_ident #ob_type_generics
        where
            #(#input_predicates,)*
            #(#field_tys: ::muon::Observe,)*
            #(#general_predicates,)*
        {
            type Target = ::muon::helper::Pointer<#head>;
            fn deref(&self) -> &Self::Target {
                &self.ptr
            }
        }

        #[automatically_derived]
        impl #ob_impl_generics ::std::ops::DerefMut
        for #ob_ident #ob_type_generics
        where
            #(#input_predicates,)*
            #(#field_tys: ::muon::Observe,)*
            #(#general_predicates,)*
        {
            fn deref_mut(&mut self) -> &mut Self::Target {
                #(#if_has_variant
                    self.mutated = true;
                    self.variant = #ob_variant_ident::__Unknown;
                )*
                &mut self.ptr
            }
        }

        #[automatically_derived]
        impl #ob_impl_generics ::muon::helper::QuasiObserver
        for #ob_ident #ob_type_generics
        where
            #(#input_predicates,)*
            #(#field_tys: ::muon::Observe,)*
            #(#general_predicates,)*
            #head: ::muon::helper::AsDeref<#depth>,
            #depth: ::muon::helper::Unsigned,
        {
            type Head = #head;
            type OuterDepth = ::muon::helper::Succ<::muon::helper::Zero>;
            type InnerDepth = #depth;

            fn invalidate(this: &mut Self) {
                #(#if_has_variant
                    this.mutated = true;
                    this.variant = #ob_variant_ident::__Unknown;
                )*
            }
        }

        #[automatically_derived]
        impl #ob_impl_generics ::muon::observe::Observer
        for #ob_ident #ob_type_generics
        where
            #(#input_predicates,)*
            #(#skipped_tys: #ob_lt,)*
            #(#field_tys: ::muon::Observe,)*
            #(#general_predicates,)*
            #head: ::muon::helper::AsDeref<#depth, Target = #input_ident #input_type_generics>,
            #depth: ::muon::helper::Unsigned,
        {
            unsafe fn observe(head: *mut #head) -> Self {
                unsafe {
                    let __ptr = ::muon::helper::AsDerefPtrExt::as_deref_ptr::<#depth>(head);
                    Self {
                        #(#if_has_variant mutated: false,)*
                        #(#if_has_initial initial: #ob_initial_ident::new(&*__ptr),)*
                        #(#if_has_variant variant: #ob_variant_ident::observe(__ptr),)*
                        ptr: ::muon::helper::Pointer::new_unchecked(head),
                        phantom: ::std::marker::PhantomData,
                    }
                }
            }

            unsafe fn relocate(this: &mut Self, head: *mut #head) {
                #(#if_has_variant
                    let __ptr = unsafe { ::muon::helper::AsDerefPtrExt::as_deref_ptr::<#depth>(head) };
                    unsafe { this.variant.relocate(__ptr) }
                )*
                unsafe { ::muon::helper::Pointer::set_unchecked(this, head) };
            }
        }

        #[automatically_derived]
        impl #ob_impl_generics ::muon::observe::SerializeObserver
        for #ob_ident #ob_type_generics
        where
            #input_serialize_predicates
            #(#input_predicates,)*
            #(#skipped_tys: #ob_lt,)*
            #(#field_tys: ::muon::Observe,)*
            #(#general_predicates,)*
            #head: ::muon::helper::AsDeref<#depth, Target = #input_ident #input_type_generics>,
            #depth: ::muon::helper::Unsigned,
            #(#ob_field_tys: ::muon::observe::SerializeObserver,)*
        {
            fn flush(this: &mut Self) -> ::muon::Mutations {
                let value = this.ptr.as_deref();
                #ob_flush_prefix_stmt
                #(#if_has_variant
                    if !this.mutated {
                        return this.variant.flush(value);
                    }
                    this.mutated = false;
                    this.variant = #ob_variant_ident::__Unknown;
                )*
                #ob_flush_suffix_stmt
            }

            fn flat_flush(this: &mut Self) -> ::muon::Mutations {
                let value = this.ptr.as_deref();
                #ob_flush_prefix_stmt
                #(#if_has_variant
                    if !this.mutated {
                        return this.variant.flat_flush(value);
                    }
                    this.mutated = false;
                    this.variant = #ob_variant_ident::__Unknown;
                )*
                #ob_flush_suffix_stmt
            }
        }

        #[automatically_derived]
        impl #input_impl_generics ::muon::Observe
        for #input_ident #input_type_generics
        where
            #self_serialize_predicates
            #(#input_predicates,)*
            #(#field_tys: ::muon::Observe,)*
            #(#general_predicates,)*
        {
            type Observer<#ob_lt, #head, #depth> = #ob_ident #ob_type_generics
            where
                Self: #ob_lt,
                #(#field_tys: #ob_lt,)*
                #depth: ::muon::helper::Unsigned,
                #head: ::muon::helper::AsDerefMut<#depth, Target = Self> + ?Sized + #ob_lt;
            type Spec = ::muon::observe::DefaultSpec;
        }
    };

    for path in &input_meta.derive.1 {
        // We just assume what the user wants is one of the standard formatting traits.
        if FMT_TRAITS.iter().any(|name| path.is_ident(name)) {
            output.extend(quote! {
                #[automatically_derived]
                impl #ob_impl_generics ::std::fmt::#path
                for #ob_ident #ob_type_generics
                where
                    #(#input_predicates,)*
                    #(#field_tys: ::muon::Observe,)*
                    #(#general_predicates,)*
                    #head: ::muon::helper::AsDeref<#depth, Target = #input_ident #input_type_generics>,
                    #depth: ::muon::helper::Unsigned,
                {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        ::std::fmt::#path::fmt(self.as_deref(), f)
                    }
                }
            });
        } else if path.is_ident("PartialEq") {
            output.extend(quote! {
                #[automatically_derived]
                impl #ob_impl_generics ::std::cmp::PartialEq
                for #ob_ident #ob_type_generics
                where
                    #(#input_predicates,)*
                    #(#field_tys: ::muon::Observe,)*
                    #(#general_predicates,)*
                    #head: ::muon::helper::AsDeref<#depth, Target = #input_ident #input_type_generics>,
                    #depth: ::muon::helper::Unsigned,
                {
                    fn eq(&self, other: &Self) -> bool {
                        self.as_deref().eq(other.as_deref())
                    }
                }
            });
        } else if path.is_ident("Eq") {
            output.extend(quote! {
                #[automatically_derived]
                impl #ob_impl_generics ::std::cmp::Eq
                for #ob_ident #ob_type_generics
                where
                    #(#input_predicates,)*
                    #(#field_tys: ::muon::Observe,)*
                    #(#general_predicates,)*
                    #head: ::muon::helper::AsDeref<#depth, Target = #input_ident #input_type_generics>,
                    #depth: ::muon::helper::Unsigned,
                {}
            });
        } else if path.is_ident("PartialOrd") {
            output.extend(quote! {
                #[automatically_derived]
                impl #ob_impl_generics ::std::cmp::PartialOrd
                for #ob_ident #ob_type_generics
                where
                    #(#input_predicates,)*
                    #(#field_tys: ::muon::Observe,)*
                    #(#general_predicates,)*
                    #head: ::muon::helper::AsDeref<#depth, Target = #input_ident #input_type_generics>,
                    #depth: ::muon::helper::Unsigned,
                {
                    fn partial_cmp(&self, other: &Self) -> ::std::option::Option<::std::cmp::Ordering> {
                        self.as_deref().partial_cmp(other.as_deref())
                    }
                }
            });
        } else if path.is_ident("Ord") {
            output.extend(quote! {
                #[automatically_derived]
                impl #ob_impl_generics ::std::cmp::Ord
                for #ob_ident #ob_type_generics
                where
                    #(#input_predicates,)*
                    #(#field_tys: ::muon::Observe,)*
                    #(#general_predicates,)*
                    #head: ::muon::helper::AsDeref<#depth, Target = #input_ident #input_type_generics>,
                    #depth: ::muon::helper::Unsigned,
                {
                    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
                        self.as_deref().cmp(other.as_deref())
                    }
                }
            });
        }
    }

    if input_meta.expose {
        output
    } else {
        quote! {
            const _: () = {
                #output
            };
        }
    }
}
