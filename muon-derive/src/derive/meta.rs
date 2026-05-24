use heck::{ToKebabCase, ToLowerCamelCase, ToShoutyKebabCase, ToShoutySnakeCase, ToSnakeCase, ToUpperCamelCase};
use proc_macro2::TokenStream;
use syn::parse::{Parse, Parser};
use syn::parse_quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;

use crate::derive::snapshot::{derive_default, derive_snapshot};

pub struct MetaArgument {
    ident: syn::Ident,
    args: Option<(syn::token::Paren, TokenStream)>,
    value: Option<(syn::Token![=], syn::Expr)>,
}

impl Parse for MetaArgument {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident: syn::Ident = input.parse()?;
        let args = if input.peek(syn::token::Paren) {
            let content;
            let paren = syn::parenthesized!(content in input);
            let args = content.parse()?;
            Some((paren, args))
        } else {
            None
        };
        let value = if input.peek(syn::Token![=]) {
            let eq_token: syn::Token![=] = input.parse()?;
            let expr: syn::Expr = input.parse()?;
            Some((eq_token, expr))
        } else {
            None
        };
        Ok(MetaArgument { ident, args, value })
    }
}

pub struct GeneralImpl {
    pub ob_ident: syn::Ident,
    pub spec_ident: syn::Ident,
    pub bounds: Punctuated<syn::TypeParamBound, syn::Token![+]>,
    pub extra_derive: fn(&syn::DeriveInput) -> TokenStream,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AttributeKind {
    Item,
    Field,
    Variant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeriveKind {
    Struct,
    Enum,
    Union,
}

#[derive(Default)]
pub struct SerdeMeta {
    pub flatten: bool,
    pub untagged: bool,
    pub tag: Option<syn::Expr>,
    pub content: Option<syn::Expr>,
    pub rename: Option<syn::Expr>,
    pub rename_all: RenameRule,
    pub rename_all_fields: RenameRule,
    pub skip: bool,
    pub skip_serializing: bool,
    pub skip_serializing_if: Option<syn::Path>,
}

#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub enum RenameRule {
    #[default]
    None,
    LowerCase,
    UpperCase,
    PascalCase,
    CamelCase,
    SnakeCase,
    ScreamingSnakeCase,
    KebabCase,
    ScreamingKebabCase,
}

const RENAME_RULES: &[(&str, RenameRule)] = &[
    ("lowercase", RenameRule::LowerCase),
    ("UPPERCASE", RenameRule::UpperCase),
    ("PascalCase", RenameRule::PascalCase),
    ("camelCase", RenameRule::CamelCase),
    ("snake_case", RenameRule::SnakeCase),
    ("SCREAMING_SNAKE_CASE", RenameRule::ScreamingSnakeCase),
    ("kebab-case", RenameRule::KebabCase),
    ("SCREAMING-KEBAB-CASE", RenameRule::ScreamingKebabCase),
];

impl RenameRule {
    pub fn from_str(input: &str) -> Option<Self> {
        for (name, rule) in RENAME_RULES {
            if input == *name {
                return Some(*rule);
            }
        }
        None
    }

    pub fn apply(self, name: &str) -> String {
        match self {
            Self::None => name.to_string(),
            Self::LowerCase => name.to_ascii_lowercase(),
            Self::UpperCase => name.to_ascii_uppercase(),
            Self::PascalCase => name.to_upper_camel_case(),
            Self::CamelCase => name.to_lower_camel_case(),
            Self::SnakeCase => name.to_snake_case(),
            Self::ScreamingSnakeCase => name.to_shouty_snake_case(),
            Self::KebabCase => name.to_kebab_case(),
            Self::ScreamingKebabCase => name.to_shouty_kebab_case(),
        }
    }
}

#[derive(Default)]
pub struct ObserveMeta {
    pub skip: bool,
    pub general_impl: Option<GeneralImpl>,
    pub deref: Option<syn::Ident>,
    pub serde: SerdeMeta,
    pub derive: (Vec<syn::Ident>, Vec<syn::Path>),
    pub expose: bool,
    pub __variant: Vec<syn::Meta>,
    pub __initial: Vec<syn::Meta>,
}

impl ObserveMeta {
    fn parse_muon(
        &mut self,
        arg: MetaArgument,
        errors: &mut TokenStream,
        attribute_kind: AttributeKind,
        derive_kind: DeriveKind,
    ) {
        match arg.ident.to_string().as_str() {
            "noop" | "skip" => self.skip = true,
            "shallow" => {
                self.general_impl = Some(GeneralImpl {
                    ob_ident: syn::Ident::new("ShallowObserver", arg.ident.span()),
                    spec_ident: syn::Ident::new("DefaultSpec", arg.ident.span()),
                    bounds: Default::default(),
                    extra_derive: derive_default,
                });
            }
            "snapshot" => {
                self.general_impl = Some(GeneralImpl {
                    ob_ident: syn::Ident::new("SnapshotObserver", arg.ident.span()),
                    spec_ident: syn::Ident::new("SnapshotSpec", arg.ident.span()),
                    bounds: parse_quote! { ::muon::general::Snapshot },
                    extra_derive: derive_snapshot,
                });
            }
            "deref" => {
                if attribute_kind != AttributeKind::Field || derive_kind != DeriveKind::Struct {
                    errors.extend(
                        syn::Error::new(
                            arg.ident.span(),
                            "the 'deref' argument is only allowed on struct fields",
                        )
                        .to_compile_error(),
                    );
                }
                self.deref = Some(arg.ident);
            }
            "derive" => {
                if attribute_kind != AttributeKind::Item {
                    errors.extend(
                        syn::Error::new(arg.ident.span(), "the 'derive' argument is only allowed on items")
                            .to_compile_error(),
                    );
                }
                let Some((_, derive_args)) = arg.args else {
                    errors.extend(
                        syn::Error::new(
                            arg.ident.span(),
                            "the 'derive' argument requires a list of traits, e.g., derive(Debug)",
                        )
                        .to_compile_error(),
                    );
                    return;
                };
                self.derive.0.push(arg.ident);
                match Punctuated::<syn::Path, syn::Token![,]>::parse_terminated.parse2(derive_args) {
                    Ok(paths) => self.derive.1.extend(paths),
                    Err(error) => errors.extend(error.to_compile_error()),
                };
            }
            "expose" => {
                if attribute_kind != AttributeKind::Item {
                    errors.extend(
                        syn::Error::new(arg.ident.span(), "the 'expose' argument is only allowed on items")
                            .to_compile_error(),
                    );
                }
                self.expose = true;
            }
            "__variant" => {
                if derive_kind != DeriveKind::Enum {
                    errors.extend(
                        syn::Error::new(
                            arg.ident.span(),
                            "the '__variant' argument is only allowed on enum items",
                        )
                        .to_compile_error(),
                    );
                }
                let Some((_, tokens)) = arg.args else {
                    errors.extend(
                        syn::Error::new(
                            arg.ident.span(),
                            "the '__variant' argument requires meta list, e.g., __variant(derive(Debug))",
                        )
                        .to_compile_error(),
                    );
                    return;
                };
                match syn::parse2(tokens) {
                    Ok(meta) => self.__variant.push(meta),
                    Err(err) => errors.extend(err.to_compile_error()),
                }
            }
            "__initial" => {
                if derive_kind != DeriveKind::Enum {
                    errors.extend(
                        syn::Error::new(
                            arg.ident.span(),
                            "the '__initial' argument is only allowed on enum items",
                        )
                        .to_compile_error(),
                    );
                }
                let Some((_, tokens)) = arg.args else {
                    errors.extend(
                        syn::Error::new(
                            arg.ident.span(),
                            "the '__initial' argument requires meta list, e.g., __initial(derive(Debug))",
                        )
                        .to_compile_error(),
                    );
                    return;
                };
                match syn::parse2(tokens) {
                    Ok(meta) => self.__initial.push(meta),
                    Err(err) => errors.extend(err.to_compile_error()),
                }
            }
            _ => errors.extend(
                syn::Error::new(
                    arg.ident.span(),
                    "unknown argument, expected 'deref', 'shallow', 'skip' or 'snapshot'",
                )
                .to_compile_error(),
            ),
        }
    }

    // do not handle serde attributes parsing errors
    fn parse_serde(&mut self, arg: MetaArgument) {
        match (arg.ident.to_string().as_str(), arg.value.map(|(_, expr)| expr)) {
            ("flatten", _) => self.serde.flatten = true,
            ("untagged", _) => self.serde.untagged = true,
            ("tag", Some(expr)) => self.serde.tag = Some(expr),
            ("content", Some(expr)) => self.serde.content = Some(expr),
            ("rename", Some(expr)) => self.serde.rename = Some(expr),
            (
                "rename_all",
                Some(syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                })),
            ) => {
                let Some(rule) = RenameRule::from_str(&lit_str.value()) else {
                    return;
                };
                self.serde.rename_all = rule;
            }
            (
                "rename_all_fields",
                Some(syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                })),
            ) => {
                let Some(rule) = RenameRule::from_str(&lit_str.value()) else {
                    return;
                };
                self.serde.rename_all_fields = rule;
            }
            ("skip", _) => self.serde.skip = true,
            ("skip_serializing", _) => self.serde.skip_serializing = true,
            (
                "skip_serializing_if",
                Some(syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                })),
            ) => self.serde.skip_serializing_if = Some(syn::parse_str(&lit_str.value()).unwrap()),
            _ => {}
        }
    }

    pub fn parse_attrs(
        attrs: &[syn::Attribute],
        errors: &mut TokenStream,
        attribute_kind: AttributeKind,
        derive_kind: DeriveKind,
    ) -> Self {
        let mut meta = ObserveMeta::default();
        for attr in attrs {
            if attr.path().is_ident("muon") {
                let syn::Meta::List(meta_list) = &attr.meta else {
                    errors.extend(
                        syn::Error::new(attr.span(), "the 'muon' attribute must be in the form of #[muon(...)]")
                            .to_compile_error(),
                    );
                    continue;
                };
                let args = match Punctuated::<MetaArgument, syn::Token![,]>::parse_terminated
                    .parse2(meta_list.tokens.clone())
                {
                    Ok(args) => args,
                    Err(err) => {
                        errors.extend(err.to_compile_error());
                        continue;
                    }
                };
                for arg in args {
                    meta.parse_muon(arg, errors, attribute_kind, derive_kind);
                }
            } else if attr.path().is_ident("serde") {
                let syn::Meta::List(meta_list) = &attr.meta else {
                    errors.extend(
                        syn::Error::new(
                            attr.span(),
                            "the 'serde' attribute must be in the form of #[serde(...)]",
                        )
                        .to_compile_error(),
                    );
                    continue;
                };
                let args = match Punctuated::<MetaArgument, syn::Token![,]>::parse_terminated
                    .parse2(meta_list.tokens.clone())
                {
                    Ok(args) => args,
                    Err(err) => {
                        errors.extend(err.to_compile_error());
                        continue;
                    }
                };
                for arg in args {
                    meta.parse_serde(arg);
                }
            }
        }
        meta
    }
}
