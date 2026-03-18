//! WCL derive macros.
//!
//! Provides `#[derive(WclDeserialize)]` which generates a `serde::Deserialize`
//! implementation with WCL-specific attribute handling.
//!
//! # Supported field attributes
//!
//! - `#[wcl(id)]`      — map the block's inline ID to this field
//! - `#[wcl(labels)]`  — map the block's labels to this field (`Vec<String>`)
//! - `#[wcl(flatten)]` — flatten a nested block into this struct

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Fields, Ident, Meta, Type,
};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

#[proc_macro_derive(WclDeserialize, attributes(wcl))]
pub fn derive_wcl_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    TokenStream::from(expand_wcl_deserialize(input))
}

// ---------------------------------------------------------------------------
// Core expansion logic
// ---------------------------------------------------------------------------

/// Recognised `#[wcl(...)]` annotations on a field.
#[derive(Default)]
struct WclFieldAttr {
    id: bool,
    labels: bool,
    flatten: bool,
}

impl WclFieldAttr {
    fn from_attrs(attrs: &[Attribute]) -> Self {
        let mut out = WclFieldAttr::default();
        for attr in attrs {
            if !attr.path().is_ident("wcl") {
                continue;
            }
            if let Meta::List(list) = &attr.meta {
                // Parse the token stream inside wcl(...) as a comma-separated
                // list of identifiers.
                let _ = list.parse_nested_meta(|meta| {
                    if meta.path.is_ident("id") {
                        out.id = true;
                    } else if meta.path.is_ident("labels") {
                        out.labels = true;
                    } else if meta.path.is_ident("flatten") {
                        out.flatten = true;
                    }
                    Ok(())
                });
            }
        }
        out
    }
}

fn expand_wcl_deserialize(input: DeriveInput) -> TokenStream2 {
    let struct_name = &input.ident;
    let (_impl_generics, ty_generics, where_clause) =
        input.generics.split_for_impl();

    let named_fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("WclDeserialize only supports structs with named fields"),
        },
        _ => panic!("WclDeserialize only supports structs"),
    };

    // Collect per-field information.
    let mut field_names: Vec<&Ident> = Vec::new();
    let mut field_types: Vec<&Type> = Vec::new();
    let mut field_attrs: Vec<WclFieldAttr> = Vec::new();

    for field in named_fields {
        field_names.push(field.ident.as_ref().unwrap());
        field_types.push(&field.ty);
        field_attrs.push(WclFieldAttr::from_attrs(&field.attrs));
    }

    // ------------------------------------------------------------------
    // Build the helper struct fields and the construction expression.
    //
    // Mapping rules (applied by wcl_serde's Deserializer before we ever
    // see the data, so all we need here is the correct serde field name):
    //
    //   #[wcl(id)]      → serde field name "id"
    //   #[wcl(labels)]  → serde field name "labels"
    //   #[wcl(flatten)] → #[serde(flatten)] on the helper field
    //   (none)          → field name as-is (snake_case)
    // ------------------------------------------------------------------

    let mut helper_field_defs: Vec<TokenStream2> = Vec::new();
    let mut construct_fields: Vec<TokenStream2> = Vec::new();

    for ((field_name, field_ty), attr) in field_names
        .iter()
        .zip(field_types.iter())
        .zip(field_attrs.iter())
    {
        if attr.id {
            // Rename to "id" so the deserializer maps the inline block ID here.
            helper_field_defs.push(quote! {
                #[serde(rename = "id")]
                #field_name: #field_ty,
            });
        } else if attr.labels {
            // Rename to "labels" so the deserializer maps block labels here.
            helper_field_defs.push(quote! {
                #[serde(rename = "labels")]
                #field_name: #field_ty,
            });
        } else if attr.flatten {
            helper_field_defs.push(quote! {
                #[serde(flatten)]
                #field_name: #field_ty,
            });
        } else {
            helper_field_defs.push(quote! {
                #field_name: #field_ty,
            });
        }

        construct_fields.push(quote! {
            #field_name: __helper.#field_name,
        });
    }

    // Unique identifier for the helper struct to avoid name collisions.
    let helper_ident = Ident::new(
        &format!("__WclDeserializeHelper_{}", struct_name),
        struct_name.span(),
    );

    // Build a combined generics that includes 'de plus any user generics.
    let mut de_generics = input.generics.clone();
    de_generics.params.insert(
        0,
        syn::GenericParam::Lifetime(syn::LifetimeParam::new(syn::Lifetime::new(
            "'de",
            proc_macro2::Span::call_site(),
        ))),
    );
    let (de_impl_generics, _, _) = de_generics.split_for_impl();

    quote! {
        impl #de_impl_generics serde::Deserialize<'de> for #struct_name #ty_generics
        #where_clause
        {
            fn deserialize<__D>(deserializer: __D) -> ::core::result::Result<Self, __D::Error>
            where
                __D: serde::Deserializer<'de>,
            {
                // The helper is defined inside the function body so it doesn't
                // leak into the surrounding namespace and doesn't require the
                // caller's generics.
                #[allow(non_camel_case_types)]
                #[derive(serde::Deserialize)]
                struct #helper_ident {
                    #(#helper_field_defs)*
                }

                let __helper = <#helper_ident as serde::Deserialize<'de>>::deserialize(deserializer)?;

                ::core::result::Result::Ok(#struct_name {
                    #(#construct_fields)*
                })
            }
        }
    }
}
