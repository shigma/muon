use std::borrow::Cow;
use std::collections::HashSet;
use std::mem::take;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse_quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;

use crate::derive::meta::{AttributeKind, DeriveKind, GeneralImpl, ObserveMeta};
use crate::derive::snapshot::derive_noop_snapshot;

mod r#enum;
mod meta;
mod snapshot;
mod r#struct;

pub const FMT_TRAITS: &[&str] = &[
    "Binary", "Display", "LowerExp", "LowerHex", "Octal", "Pointer", "UpperExp", "UpperHex",
];

pub fn derive_observe(mut input: syn::DeriveInput) -> TokenStream {
    let mut errors = quote! {};
    let mut input_meta = ObserveMeta::parse_attrs(
        &input.attrs,
        &mut errors,
        AttributeKind::Item,
        match &input.data {
            syn::Data::Struct(_) => DeriveKind::Struct,
            syn::Data::Enum(_) => DeriveKind::Enum,
            syn::Data::Union(_) => DeriveKind::Union,
        },
    );

    if input_meta.general_impl.is_none()
        && let syn::Data::Struct(syn::DataStruct { fields, .. }) = &input.data
        && fields.iter().all(|field| {
            let meta = ObserveMeta::parse_attrs(&field.attrs, &mut errors, AttributeKind::Field, DeriveKind::Struct);
            meta.skip || meta.serde.skip || meta.serde.skip_serializing
        })
    {
        input_meta.general_impl = Some(GeneralImpl {
            ob_ident: format_ident!("NoopObserver"),
            spec_ident: format_ident!("SnapshotSpec"),
            bounds: Default::default(),
            extra_derive: derive_noop_snapshot,
        });
    }

    if !errors.is_empty() {
        return errors;
    }

    if let Some(GeneralImpl {
        ob_ident,
        spec_ident,
        bounds,
        extra_derive,
    }) = input_meta.general_impl
    {
        let input_ident = &input.ident;
        let mut generics_visitor = GenericsVisitor::default();
        generics_visitor.visit_derive_input(&input);
        let head = generics_visitor.allocate_ty(parse_quote!(S));
        let depth = generics_visitor.allocate_ty(parse_quote!(N));
        let ob_lt = generics_visitor.allocate_lt(parse_quote!('ob));
        let mut where_predicates = match take(&mut input.generics.where_clause) {
            Some(where_clause) => where_clause.predicates,
            None => Default::default(),
        };
        if !bounds.is_empty() {
            where_predicates.push(parse_quote! { Self: #bounds });
        }
        let extra = extra_derive(&input);
        let (impl_generics, type_generics, _) = input.generics.split_for_impl();
        return quote! {
            #extra

            #[automatically_derived]
            impl #impl_generics ::muon::Observe for #input_ident #type_generics where #where_predicates {
                type Observer<#ob_lt, #head, #depth>
                    = ::muon::general::#ob_ident<#ob_lt, #head, #depth>
                where
                    Self: #ob_lt,
                    #depth: ::muon::helper::Unsigned,
                    #head: ::muon::helper::AsDerefMut<#depth, Target = Self> + ?Sized + #ob_lt;

                type Spec = ::muon::observe::#spec_ident;
            }
        };
    }

    match &input.data {
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Named(syn::FieldsNamed { named, .. }),
            ..
        }) => r#struct::derive_observe_for_struct(&input, named, &input_meta, true),
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Unnamed(syn::FieldsUnnamed { unnamed, .. }),
            ..
        }) => r#struct::derive_observe_for_struct(&input, unnamed, &input_meta, false),
        syn::Data::Enum(syn::DataEnum { variants, .. }) => {
            r#enum::derive_observe_for_enum(&input, variants, &input_meta)
        }
        _ => syn::Error::new(input.span(), "Observe can only be derived for structs or enums").to_compile_error(),
    }
}

#[derive(Default)]
struct GenericsVisitor<'i> {
    ty_idents: HashSet<Cow<'i, syn::Ident>>,
    lt_idents: HashSet<Cow<'i, syn::Ident>>,
}

impl<'i> GenericsVisitor<'i> {
    fn contains_ty(&self, ident: &syn::Ident) -> bool {
        self.ty_idents.contains(ident)
    }

    fn contains_lt(&self, lifetime: &syn::Lifetime) -> bool {
        self.lt_idents.contains(&lifetime.ident)
    }

    fn allocate_ty(&mut self, ident: syn::Ident) -> syn::Ident {
        let mut ident: Cow<'i, syn::Ident> = Cow::Owned(ident);
        while !self.ty_idents.insert(ident.clone()) {
            let new_ident = format_ident!("_{}", ident);
            ident = Cow::Owned(new_ident);
        }
        ident.into_owned()
    }

    fn allocate_lt(&mut self, mut lifetime: syn::Lifetime) -> syn::Lifetime {
        let mut ident: Cow<'i, syn::Ident> = Cow::Owned(lifetime.ident);
        while !self.lt_idents.insert(ident.clone()) {
            let new_ident = format_ident!("_{}", ident);
            ident = Cow::Owned(new_ident);
        }
        lifetime.ident = ident.into_owned();
        lifetime
    }
}

impl<'i, 'ast: 'i> Visit<'ast> for GenericsVisitor<'i> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(ident) = path.get_ident() {
            self.ty_idents.insert(Cow::Borrowed(ident));
        }
    }

    fn visit_lifetime_param(&mut self, lt_param: &'ast syn::LifetimeParam) {
        self.lt_idents.insert(Cow::Borrowed(&lt_param.lifetime.ident));
    }
}

struct GenericsDetector<'i> {
    is_detected: bool,
    params: &'i Punctuated<syn::GenericParam, syn::Token![,]>,
}

impl<'i> GenericsDetector<'i> {
    fn detect(ty: &syn::Type, generics: &'i syn::Generics) -> bool {
        let mut checker = GenericsDetector {
            is_detected: false,
            params: &generics.params,
        };
        syn::visit::visit_type(&mut checker, ty);
        checker.is_detected
    }
}

impl<'i> Visit<'_> for GenericsDetector<'i> {
    fn visit_type_path(&mut self, type_path: &syn::TypePath) {
        if type_path.qself.is_none()
            && let Some(ident) = type_path.path.get_ident()
        {
            for param in self.params {
                match param {
                    syn::GenericParam::Type(ty_param) => {
                        if &ty_param.ident == ident {
                            self.is_detected = true;
                        }
                    }
                    syn::GenericParam::Lifetime(_lt_param) => {}
                    syn::GenericParam::Const(const_param) => {
                        if &const_param.ident == ident {
                            self.is_detected = true;
                        }
                    }
                }
            }
        }
        syn::visit::visit_type_path(self, type_path);
    }

    fn visit_lifetime(&mut self, lifetime: &syn::Lifetime) {
        for param in self.params {
            if let syn::GenericParam::Lifetime(lt_param) = param
                && &lt_param.lifetime == lifetime
            {
                self.is_detected = true;
            }
        }
    }
}
