//! Provide proc macros used by `gpui_tea`.

mod expand;
mod parser;
mod spec;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Derive child-model forwarding helpers for a composite model struct.
///
/// The derive supports structs with named fields. It generates hidden aggregate helpers
/// `__composite_init`, `__composite_update`, and `__composite_subscriptions`, plus one hidden
/// `<field>_view` helper per field marked with `#[child(...)]`.
///
/// Supported attributes:
///
/// - `#[composite(message = ParentMsg)]` on the struct to declare the parent message type
/// - `#[child(path = "...", lift = ..., extract = ...)]` on child fields to define:
///   - the child path segment used for key scoping
///   - a function or constructor that lifts child messages into the parent message type
///   - a function that extracts a child message or returns the original parent message
///
/// Child subscriptions must remain unique after path scoping. The generated
/// `__composite_subscriptions` helper panics if two child subscriptions collide.
#[proc_macro_derive(Composite, attributes(composite, child))]
pub fn derive_composite(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match parser::parse_composite(input).map(expand::expand_composite) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
