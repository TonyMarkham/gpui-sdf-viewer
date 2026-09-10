//! Inert marker attributes consumed by the Soul semantic indexer.
//!
//! The indexer greps source files for `#[soul(id = "...")]` annotations to
//! link code locations to documentation. This crate defines the attribute as
//! a no-op proc-macro so the compiler accepts it and discards the metadata.

use proc_macro::TokenStream;

/// Documentation-only attribute: the Soul indexer reads `#[soul(id = "...",
/// key = "value")]` markers; this macro expands to the annotated item
/// unchanged.
#[proc_macro_attribute]
pub fn soul(_metadata: TokenStream, item: TokenStream) -> TokenStream {
    item
}
