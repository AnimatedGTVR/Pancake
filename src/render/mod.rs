pub mod aero;

pub use aero::AeroRenderer;

// Rendering is done inline in each backend to avoid complex generic bounds.
// This module houses shader infrastructure and effect passes (see aero.rs).
//
// Future: extract a generic render_output<R: Renderer>(...)  once the Aero
// blur pipeline is implemented — the gist will be:
//   1. backend.bind() → (renderer, framebuffer)
//   2. AeroRenderer::begin_frame(renderer) — build blurred-bg texture
//   3. frame.clear(deep blue-grey base)
//   4. draw blurred background quads on glass surfaces
//   5. draw_render_elements — composite all windows on top
//   6. frame.finish()
