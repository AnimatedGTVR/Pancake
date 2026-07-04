pub mod aero;
pub mod cursor;

pub use aero::AeroRenderer;

// ── Unified render-element enum for both backends ─────────────────────────────
//
// `<=` binds this enum to a concrete renderer type (GlesRenderer) instead of
// generating a generic impl.  Both inner types must implement RenderElement for
// the same concrete renderer.
use smithay::{
    backend::renderer::{
        element::{
            render_elements,
            surface::WaylandSurfaceRenderElement,
            texture::TextureRenderElement,
        },
        gles::{GlesRenderer, GlesTexture},
    },
    desktop::space::SpaceRenderElements,
};

render_elements! {
    pub PancakeElements <= GlesRenderer;
    Space  = SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>,
    Cursor = TextureRenderElement<GlesTexture>,
}
