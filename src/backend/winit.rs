use ::winit::platform::pump_events::PumpStatus;
/// Winit backend — run Pancake as a window inside an existing compositor.
///
/// Launch: `pancake --winit`
///
/// This is the development/test backend. No TTY or GPU access needed.
/// Listens on `WAYLAND_DISPLAY=wayland-pancake` by default.
use smithay::{
    backend::{
        renderer::{gles::GlesRenderer, utils::draw_render_elements, Color32F, Frame, Renderer},
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

use crate::state::{ClientState, PancakeState};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // ── Wayland display ──────────────────────────────────────────────────────
    let mut display: Display<PancakeState> = Display::new()?;
    let mut state = PancakeState::new(&display);

    // ── Winit backend ────────────────────────────────────────────────────────
    let (mut winit_backend, mut winit_events) = winit::init::<GlesRenderer>()?;

    // ── Synthetic output ─────────────────────────────────────────────────────
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
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    // ── Wayland listening socket ─────────────────────────────────────────────
    let listener = ListeningSocket::bind("wayland-pancake")
        .or_else(|_| ListeningSocket::bind_auto("wayland", 1..33))?;

    let name = listener
        .socket_name()
        .and_then(|s| s.to_str())
        .unwrap_or("wayland-pancake");
    info!("Pancake listening on WAYLAND_DISPLAY={name}");
    std::env::set_var("WAYLAND_DISPLAY", name);

    info!("Running Pancake (Winit). Super+Q = close window, Super+Escape = quit.");

    // ── Main loop ────────────────────────────────────────────────────────────
    loop {
        let status = winit_events.dispatch_new_events(|event| match event {
            WinitEvent::Input(ev) => state.process_input_event(ev),
            WinitEvent::Resized { size, .. } => {
                let new_mode = Mode {
                    size: (size.w as i32, size.h as i32).into(),
                    refresh: 60_000,
                };
                output.change_current_state(Some(new_mode), None, None, None);
            }
            WinitEvent::CloseRequested => {
                info!("Window closed.");
                std::process::exit(0);
            }
            _ => {}
        });

        if matches!(status, PumpStatus::Exit(_)) {
            break;
        }

        // ── Render ───────────────────────────────────────────────────────────
        let size = winit_backend.window_size();
        let damage = Rectangle::from_size(size);

        // Render block: renderer + framebuffer must drop before submit().
        {
            let (renderer, mut framebuffer) = winit_backend.bind()?;

            // Gather render elements from the mapped desktop space so layout,
            // popups, and output membership match the real compositor state.
            let elements = state
                .space
                .render_elements_for_output(renderer, &output, 1.0)
                .unwrap_or_default();

            let mut frame = renderer.render(&mut framebuffer, size, Transform::Flipped180)?;

            // Aero-COSMIC dark base (deep midnight blue, not pure black).
            frame.clear(Color32F::new(0.05, 0.05, 0.08, 1.0), &[damage])?;
            draw_render_elements(&mut frame, 1.0, &elements, &[damage])?;
            let _ = frame.finish()?;
        }

        winit_backend.submit(Some(&[damage]))?;

        // ── Frame callbacks ──────────────────────────────────────────────────
        let time_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u32;

        for toplevel in state.xdg_shell_state.toplevel_surfaces() {
            with_surface_tree_downward(
                toplevel.wl_surface(),
                (),
                |_, _, _| TraversalAction::DoChildren(()),
                |_, surf_state, _| {
                    for cb in surf_state
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .current()
                        .frame_callbacks
                        .drain(..)
                    {
                        cb.done(time_ms);
                    }
                },
                |_, _, _| true,
            );
        }

        // ── Accept new clients ───────────────────────────────────────────────
        while let Some(stream) = listener.accept().ok().flatten() {
            display
                .handle()
                .insert_client(stream, Arc::new(ClientState::default()))
                .ok();
        }

        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;
    }

    Ok(())
}
