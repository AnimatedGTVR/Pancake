use ::winit::platform::pump_events::PumpStatus;
/// Winit backend — run Pancake as a window inside an existing compositor.
use smithay::{
    backend::{
        renderer::{
            element::{
                texture::{TextureBuffer, TextureRenderElement},
                Kind,
            },
            gles::{GlesRenderer, GlesTexture},
            utils::draw_render_elements,
            Color32F, Frame, ImportMem, Renderer,
        },
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::wayland_server::Display,
    utils::{Rectangle, Transform},
    wayland::compositor::{with_surface_tree_downward, SurfaceAttributes, TraversalAction},
};
use std::sync::Arc;
use tracing::info;
use wayland_server::ListeningSocket;

use crate::render::{cursor, PancakeElements};
use crate::state::{ClientState, PancakeState};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut display: Display<PancakeState> = Display::new()?;
    let mut state = PancakeState::new(&display);

    let (mut winit_backend, mut winit_events) = winit::init::<GlesRenderer>()?;

    // ── Import cursor as a TextureBuffer ─────────────────────────────────────
    let cursor_img = cursor::load_default();
    let cursor_hotspot = (cursor_img.hot_x, cursor_img.hot_y);
    let cursor_buffer: Option<TextureBuffer<GlesTexture>> = winit_backend
        .renderer()
        .import_memory(
            &cursor_img.pixels,
            smithay::backend::allocator::Fourcc::Abgr8888,
            (cursor_img.width as i32, cursor_img.height as i32).into(),
            false,
        )
        .ok()
        .map(|tex| TextureBuffer::from_texture(winit_backend.renderer(), tex, 1, Transform::Normal, None));
    if cursor_buffer.is_none() {
        tracing::warn!("Winit: cursor texture import failed; cursor will not be visible");
    }

    let win_sz = winit_backend.window_size();
    state.cursor_pos = ((win_sz.w / 2) as f64, (win_sz.h / 2) as f64).into();

    // ── Synthetic output ──────────────────────────────────────────────────────
    let window_size = winit_backend.window_size();
    let mode = Mode {
        size: (window_size.w as i32, window_size.h as i32).into(),
        refresh: 60_000,
    };
    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Pancake".into(),
            model: "Winit".into(),
        },
    );
    output.change_current_state(Some(mode), Some(Transform::Normal), None, Some((0, 0).into()));
    output.set_preferred(mode);
    output.create_global::<PancakeState>(&display.handle());
    state.space.map_output(&output, (0, 0));

    let listener = ListeningSocket::bind("wayland-pancake")
        .or_else(|_| ListeningSocket::bind_auto("wayland", 1..33))?;
    let name = listener.socket_name().and_then(|s| s.to_str()).unwrap_or("wayland-pancake");
    info!("Pancake listening on WAYLAND_DISPLAY={name}");
    std::env::set_var("WAYLAND_DISPLAY", name);
    info!("Running Pancake (Winit). Super+Q = close window, Super+Escape = quit.");

    loop {
        let status = winit_events.dispatch_new_events(|event| match event {
            WinitEvent::Input(ev) => state.process_input_event(ev),
            WinitEvent::Resized { size, .. } => {
                let new_mode = Mode { size: (size.w as i32, size.h as i32).into(), refresh: 60_000 };
                output.change_current_state(Some(new_mode), None, None, None);
            }
            WinitEvent::CloseRequested => { std::process::exit(0); }
            _ => {}
        });
        if matches!(status, PumpStatus::Exit(_)) { break; }

        state.maybe_reload_config();

        let size   = winit_backend.window_size();
        let damage = Rectangle::from_size(size);

        {
            let (renderer, mut framebuffer) = winit_backend.bind()?;

            // Space elements (windows)
            let space_elems = state.space
                .render_elements_for_output(renderer, &output, 1.0)
                .unwrap_or_default();

            // Build cursor render element
            let cursor_elem: Option<TextureRenderElement<GlesTexture>> =
                cursor_buffer.as_ref().map(|buf| {
                    let x = state.cursor_pos.x - cursor_hotspot.0 as f64;
                    let y = state.cursor_pos.y - cursor_hotspot.1 as f64;
                    TextureRenderElement::from_texture_buffer(
                        smithay::utils::Point::<f64, smithay::utils::Physical>::from((x, y)),
                        buf,
                        None, None, None,
                        Kind::Cursor,
                    )
                });

            // Merge into PancakeElements (cursor on top = pushed last)
            let mut all: Vec<PancakeElements> = space_elems
                .into_iter()
                .map(PancakeElements::Space)
                .collect();
            if let Some(ce) = cursor_elem {
                all.push(PancakeElements::Cursor(ce));
            }

            // Aero blur pipeline
            state.renderer.begin_frame(renderer, size.w as u32, size.h as u32);

            let mut frame = renderer.render(&mut framebuffer, size, Transform::Flipped180)?;
            if state.renderer.blurred_background().is_some() {
                state.renderer.draw_background(&mut frame)?;
            } else {
                frame.clear(Color32F::new(0.05, 0.08, 0.18, 1.0), &[damage])?;
            }

            draw_render_elements(&mut frame, 1.0, &all, &[damage])?;
            let _ = frame.finish()?;
        }

        winit_backend.submit(Some(&[damage]))?;

        // Frame callbacks
        let time_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u32;
        for toplevel in state.xdg_shell_state.toplevel_surfaces() {
            with_surface_tree_downward(
                toplevel.wl_surface(), (),
                |_, _, _| TraversalAction::DoChildren(()),
                |_, surf_state, _| {
                    for cb in surf_state.cached_state.get::<SurfaceAttributes>()
                        .current().frame_callbacks.drain(..)
                    { cb.done(time_ms); }
                },
                |_, _, _| true,
            );
        }

        while let Some(stream) = listener.accept().ok().flatten() {
            display.handle().insert_client(stream, Arc::new(ClientState::default())).ok();
        }
        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;
    }

    Ok(())
}
