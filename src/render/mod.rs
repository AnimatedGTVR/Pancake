pub mod aero;
pub mod cursor;
pub mod borders;

pub use aero::AeroRenderer;

// ── Unified render-element enum for both backends ─────────────────────────────
use smithay::{
    backend::renderer::{
        element::{
            render_elements,
            solid::SolidColorRenderElement,
            surface::WaylandSurfaceRenderElement,
            texture::TextureRenderElement,
        },
        gles::{GlesRenderer, GlesTexture},
    },
    desktop::space::SpaceRenderElements,
};

render_elements! {
    pub PancakeElements<=GlesRenderer>;
    Space  = SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>,
    Layer  = WaylandSurfaceRenderElement<GlesRenderer>,
    Cursor = TextureRenderElement<GlesTexture>,
    Border = SolidColorRenderElement,
}
