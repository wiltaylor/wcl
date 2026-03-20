//! WCL derive macros.
//!
//! Provides `#[derive(WclDeserialize)]` which generates a `serde::Deserialize`
//! implementation with WCL-specific attribute handling.
//!
//! Provides `#[derive(WclSchema)]` which generates a `fn wcl_schema() -> &'static str`
//! returning valid WCL schema text from a Rust struct.
//!
//! # Supported field attributes (WclDeserialize)
//!
//! - `#[wcl(id)]`      — map the block's inline ID to this field
//! - `#[wcl(labels)]`  — map the block's labels to this field (`Vec<String>`)
//! - `#[wcl(flatten)]` — flatten a nested block into this struct
//!
//! # Supported attributes (WclSchema)
//!
//! - `#[wcl(schema_name = "name")]` — override schema name (default: snake_case struct name)
//! - `#[wcl(open)]`                 — schema allows extra fields
//! - `#[wcl(optional)]`             — field is optional (`@optional`)
//! - `#[wcl(default = "value")]`    — field has a default value (`@default(value)`)
//! - `#[wcl(validate(min = N, max = N))]` — adds `@validate(...)` decorator

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Attribute, Data, DeriveInput, Fields, Ident, Lit, Meta, Type};

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
    let (_impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

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

// ===========================================================================
// WclSchema derive macro
// ===========================================================================

#[proc_macro_derive(WclSchema, attributes(wcl))]
pub fn derive_wcl_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    TokenStream::from(expand_wcl_schema(input))
}

/// Recognised `#[wcl(...)]` annotations for WclSchema on a struct.
#[derive(Default)]
struct WclSchemaStructAttr {
    schema_name: Option<String>,
    open: bool,
}

/// Recognised `#[wcl(...)]` annotations for WclSchema on a field.
#[derive(Default)]
struct WclSchemaFieldAttr {
    optional: bool,
    default: Option<String>,
    validate_min: Option<String>,
    validate_max: Option<String>,
}

fn parse_schema_struct_attrs(attrs: &[Attribute]) -> WclSchemaStructAttr {
    let mut out = WclSchemaStructAttr::default();
    for attr in attrs {
        if !attr.path().is_ident("wcl") {
            continue;
        }
        if let Meta::List(list) = &attr.meta {
            let _ = list.parse_nested_meta(|meta| {
                if meta.path.is_ident("open") {
                    out.open = true;
                } else if meta.path.is_ident("schema_name") {
                    let value = meta.value()?;
                    let lit: Lit = value.parse()?;
                    if let Lit::Str(s) = lit {
                        out.schema_name = Some(s.value());
                    }
                }
                Ok(())
            });
        }
    }
    out
}

fn parse_schema_field_attrs(attrs: &[Attribute]) -> WclSchemaFieldAttr {
    let mut out = WclSchemaFieldAttr::default();
    for attr in attrs {
        if !attr.path().is_ident("wcl") {
            continue;
        }
        if let Meta::List(list) = &attr.meta {
            let _ = list.parse_nested_meta(|meta| {
                if meta.path.is_ident("optional") {
                    out.optional = true;
                } else if meta.path.is_ident("default") {
                    let value = meta.value()?;
                    let lit: Lit = value.parse()?;
                    if let Lit::Str(s) = lit {
                        out.default = Some(s.value());
                    }
                } else if meta.path.is_ident("validate") {
                    // Parse validate(min = N, max = N) from the nested content
                    // This is tricky with parse_nested_meta, handle as string for now
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let inner: proc_macro2::TokenStream = content.parse()?;
                    let inner_str = inner.to_string();
                    for part in inner_str.split(',') {
                        let part = part.trim();
                        if let Some(val) = part.strip_prefix("min =") {
                            out.validate_min = Some(val.trim().to_string());
                        } else if let Some(val) = part.strip_prefix("max =") {
                            out.validate_max = Some(val.trim().to_string());
                        } else if let Some(val) = part.strip_prefix("min=") {
                            out.validate_min = Some(val.trim().to_string());
                        } else if let Some(val) = part.strip_prefix("max=") {
                            out.validate_max = Some(val.trim().to_string());
                        }
                    }
                }
                Ok(())
            });
        }
    }
    out
}

fn rust_type_to_wcl(ty: &Type) -> String {
    let ty_str = quote!(#ty).to_string().replace(' ', "");
    match ty_str.as_str() {
        "String" | "&str" => "string".into(),
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "isize" | "usize" => {
            "int".into()
        }
        "f32" | "f64" => "float".into(),
        "bool" => "bool".into(),
        _ => {
            // Handle Option<T>, Vec<T>, HashMap<K,V>
            if let Some(inner) = try_extract_generic(&ty_str, "Option") {
                rust_type_str_to_wcl(&inner)
            } else if let Some(inner) = try_extract_generic(&ty_str, "Vec") {
                format!("list({})", rust_type_str_to_wcl(&inner))
            } else if let Some(inner) = try_extract_generic(&ty_str, "HashMap") {
                // Split on first comma
                if let Some(comma_pos) = find_top_level_comma(&inner) {
                    let k = inner[..comma_pos].trim();
                    let v = inner[comma_pos + 1..].trim();
                    format!(
                        "map({}, {})",
                        rust_type_str_to_wcl(k),
                        rust_type_str_to_wcl(v)
                    )
                } else {
                    "any".into()
                }
            } else {
                "any".into()
            }
        }
    }
}

fn rust_type_str_to_wcl(s: &str) -> String {
    match s {
        "String" | "&str" => "string".into(),
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "isize" | "usize" => {
            "int".into()
        }
        "f32" | "f64" => "float".into(),
        "bool" => "bool".into(),
        _ => {
            if let Some(inner) = try_extract_generic(s, "Option") {
                rust_type_str_to_wcl(&inner)
            } else if let Some(inner) = try_extract_generic(s, "Vec") {
                format!("list({})", rust_type_str_to_wcl(&inner))
            } else if let Some(inner) = try_extract_generic(s, "HashMap") {
                if let Some(comma_pos) = find_top_level_comma(&inner) {
                    let k = inner[..comma_pos].trim();
                    let v = inner[comma_pos + 1..].trim();
                    format!(
                        "map({}, {})",
                        rust_type_str_to_wcl(k),
                        rust_type_str_to_wcl(v)
                    )
                } else {
                    "any".into()
                }
            } else {
                "any".into()
            }
        }
    }
}

fn try_extract_generic(s: &str, wrapper: &str) -> Option<String> {
    let prefix = format!("{}<", wrapper);
    if s.starts_with(&prefix) && s.ends_with('>') {
        Some(s[prefix.len()..s.len() - 1].to_string())
    } else {
        None
    }
}

fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

fn expand_wcl_schema(input: DeriveInput) -> TokenStream2 {
    let struct_name = &input.ident;
    let struct_attrs = parse_schema_struct_attrs(&input.attrs);

    let schema_name = struct_attrs
        .schema_name
        .unwrap_or_else(|| to_snake_case(&struct_name.to_string()));

    let named_fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("WclSchema only supports structs with named fields"),
        },
        _ => panic!("WclSchema only supports structs"),
    };

    let mut field_lines = Vec::new();

    if struct_attrs.open {
        field_lines.push("    @open".to_string());
    }

    for field in named_fields {
        let field_name = field.ident.as_ref().unwrap().to_string();
        let field_ty = &field.ty;
        let attrs = parse_schema_field_attrs(&field.attrs);

        let wcl_type = rust_type_to_wcl(field_ty);
        let is_option = quote!(#field_ty)
            .to_string()
            .replace(' ', "")
            .starts_with("Option<");

        let mut decorators = Vec::new();
        if attrs.optional || is_option {
            decorators.push("@optional".to_string());
        }
        if let Some(ref default_val) = attrs.default {
            decorators.push(format!("@default({})", default_val));
        }
        if attrs.validate_min.is_some() || attrs.validate_max.is_some() {
            let mut validate_args = Vec::new();
            if let Some(ref min) = attrs.validate_min {
                validate_args.push(format!("min = {}", min));
            }
            if let Some(ref max) = attrs.validate_max {
                validate_args.push(format!("max = {}", max));
            }
            decorators.push(format!("@validate({})", validate_args.join(", ")));
        }

        let decorator_str = if decorators.is_empty() {
            String::new()
        } else {
            format!(" {}", decorators.join(" "))
        };

        field_lines.push(format!("    {}: {}{}", field_name, wcl_type, decorator_str));
    }

    let schema_text = format!(
        "schema \"{}\" {{\n{}\n}}\n",
        schema_name,
        field_lines.join("\n")
    );

    quote! {
        impl #struct_name {
            pub fn wcl_schema() -> &'static str {
                #schema_text
            }
        }
    }
}
