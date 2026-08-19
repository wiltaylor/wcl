//! One module per block kind wdoc renders in Rust.
//!
//! A wdoc block normally needs no Rust at all: it declares a `lower`
//! function in WCL and the backends render what that returns. The kinds
//! here are the exceptions — `@native` blocks (see [`crate::native`]),
//! which need something WCL cannot express: an asset copied into the
//! output, a spritesheet sliced, a subtree re-rendered twice, a body
//! measured before it can be placed.
//!
//! The split is by *where the kind is legal*, which is the same split the
//! stdlib declares. A module lives in [`diagram`] when its kind extends
//! `SvgBlock` and so is only ever a child of a `diagram`; it lives here
//! when the kind extends `ContentBlock` and can appear in ordinary page
//! content — including the three that do both (`icon`, `image`, and the
//! registries behind them), which every backend consults wherever they
//! land.
//!
//! Each module owns its whole kind: the registry that resolves its
//! assets, the renderers the backends call, and the diagnostics it
//! raises. They sit on [`crate::render`] for reading values off blocks
//! and on [`crate::svg`] / [`crate::html`] for emitting markup.

pub(crate) mod demo;
pub(crate) mod diagram;
pub(crate) mod file;
pub(crate) mod icons;
pub(crate) mod image;
pub(crate) mod math;
pub(crate) mod terminal;
pub(crate) mod video;
