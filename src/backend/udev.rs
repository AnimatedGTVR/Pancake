/// Udev/DRM backend — run Pancake directly on hardware from a TTY.
///
/// Launch: `pancake` (or `pancake --tty 2`)
///
/// Requires a libseat session (logind or seatd running).
use std::{fs::OpenOptions, process::Command, sync::Arc, time::Duration};

use smithay::{
    backend::{
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        session::{libseat::LibSeatSession, Event as SessionEvent, Session},
        udev::{UdevBackend, UdevEvent},
    },
    reexports::calloop::{
        timer::{TimeoutAction, Timer},
        EventLoop,
    },
    reexports::wayland_server::Display,
    wayland::compositor::{with_surface_tree_downward, SurfaceAttributes, TraversalAction},
};
use tracing::{info, warn};
use wayland_server::ListeningSocket;

use crate::state::{ClientState, PancakeState};

use super::gpu::GpuData;

pub fn run(tty: Option<u8>) -> Result<(), Box<dyn std::error::Error>> {
    let _ = tty;

    // ── Libseat session ──────────────────────────────────────────────────────
    let (mut session, notifier) = LibSeatSession::new()
        .map_err(|e| format!("libseat session failed (are you on a TTY?): {e}"))?;
    info!("Session: seat='{}'", session.seat());

    // ── Event loop + Wayland display ─────────────────────────────────────────
    let mut event_loop: EventLoop<State> = EventLoop::try_new()?;
    let mut display: Display<PancakeState> = Display::new()?;
    let mut pancake = PancakeState::new(&display);

    // ── VT switch events ─────────────────────────────────────────────────────
    event_loop
        .handle()
        .insert_source(notifier, |event, _, state: &mut State| match event {
            SessionEvent::PauseSession => {
                info!("Session paused");
                for gpu in &state.gpus {
                    // TODO: DRM device suspend (release master)
                    let _ = gpu;
                }
            }
            SessionEvent::ActivateSession => {
                info!("Session resumed — redrawing all outputs");
                for gpu in &mut state.gpus {
                    gpu.reset_outputs();
                    gpu.render_all(&state.pancake);
                }
            }
        })?;

    // ── Discover GPUs via udev ────────────────────────────────────────────────
    let udev = UdevBackend::new(session.seat())?;
    let mut gpus: Vec<GpuData> = Vec::new();

    for (_dev_id, path) in udev.device_list() {
        match GpuData::init(&mut session, &path, &pancake.space) {
            Ok((gpu_data, drm_notifier)) => {
                // Wire DRM events (VBlank, errors) into the event loop.
                let gpu_idx = gpus.len();
                event_loop
                    .handle()
                    .insert_source(drm_notifier, move |event, _meta, state: &mut State| {
                        use smithay::backend::drm::DrmEvent;
                        match event {
                            DrmEvent::VBlank(crtc) => {
                                if let Some(gpu) = state.gpus.get_mut(gpu_idx) {
                                    gpu.on_vblank(crtc);
                                    gpu.render_all(&state.pancake);
                                }
                            }
                            DrmEvent::Error(e) => warn!("DRM error on GPU {gpu_idx}: {e}"),
                        }
                    })
                    .ok();

                // Map all outputs from this GPU into the compositor space.
                for out_state in &gpu_data.outputs {
                    out_state.output.create_global::<PancakeState>(&display.handle());
                    pancake.space.map_output(&out_state.output, (0, 0));
                }

                gpus.push(gpu_data);
            }
            Err(e) => warn!("Failed to init GPU at {path:?}: {e}"),
        }
    }

    // Hot-plug handling.
    event_loop
        .handle()
        .insert_source(udev, |event, _, _state: &mut State| match event {
            UdevEvent::Added { path, .. } => info!("GPU hot-plugged: {path:?} (TODO: init)"),
            UdevEvent::Changed { .. } => {}
            UdevEvent::Removed { device_id } => warn!("GPU removed: {device_id:?}"),
        })?;

    // ── Libinput ─────────────────────────────────────────────────────────────
    let mut li = input::Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    let seat_name = session.seat();
    li.udev_assign_seat(seat_name.as_str())
        .map_err(|()| "libinput seat assignment failed")?;
    event_loop.handle().insert_source(
        LibinputInputBackend::new(li),
        |ev, _, state: &mut State| {
            state.pancake.process_input_event(ev);
        },
    )?;

    // Early Pancake does not yet have fine-grained damage scheduling. Keep a
    // gentle repaint tick so clients that map after the first DRM frame become
    // visible without waiting for another external event.
    event_loop.handle().insert_source(
        Timer::from_duration(Duration::from_millis(16)),
        |_, _, state: &mut State| {
            for gpu in &mut state.gpus {
                gpu.render_all(&state.pancake);
            }
            TimeoutAction::ToDuration(Duration::from_millis(16))
        },
    )?;

    // ── Wayland socket ────────────────────────────────────────────────────────
    let listener = ListeningSocket::bind_auto("wayland", 1..33)?;
    let socket_name = listener
        .socket_name()
        .and_then(|s| s.to_str())
        .unwrap_or("wayland-0");
    info!("Wayland socket: {socket_name}");
    std::env::set_var("WAYLAND_DISPLAY", socket_name);

    info!("Pancake running (udev/DRM). Super+Escape to quit.");

    let mut state = State { pancake, gpus };

    if state.gpus.iter().all(|gpu| gpu.outputs.is_empty()) {
        return Err("no active DRM outputs were found".into());
    }

    // Do an initial render pass on all outputs.
    for gpu in &mut state.gpus {
        gpu.render_all(&state.pancake);
    }

    spawn_wayland_client(
        "PANCAKE_STARTUP_TERMINAL",
        "foot",
        "/tmp/foot.log",
        socket_name,
    );
    spawn_wayland_client(
        "PANCAKE_STARTUP_WAYBAR",
        "waybar",
        "/tmp/waybar.log",
        socket_name,
    );

    event_loop.run(None, &mut state, |state| {
        state.pancake.maybe_reload_config();

        while let Some(stream) = listener.accept().ok().flatten() {
            info!("Accepted Wayland client on {socket_name}");
            display
                .handle()
                .insert_client(stream, Arc::new(ClientState::default()))
                .ok();
        }
        display.dispatch_clients(&mut state.pancake).ok();
        display.flush_clients().ok();
        send_frame_callbacks(&mut state.pancake);
    })?;

    Ok(())
}

// ── Combined event-loop data ──────────────────────────────────────────────────

/// Calloop's event loop data must be a single type. We bundle both the
/// compositor state and the GPU list here.
struct State {
    pancake: PancakeState,
    gpus: Vec<GpuData>,
}

use input;

fn spawn_wayland_client(env_flag: &str, command_name: &str, log_path: &str, socket_name: &str) {
    if std::env::var_os(env_flag).is_none() {
        info!("{env_flag} is not set; not starting {command_name}");
        return;
    }

    let client_log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .ok();

    let mut command = Command::new(command_name);
    command
        .env("WAYLAND_DISPLAY", socket_name)
        .env(
            "XDG_RUNTIME_DIR",
            std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/0".to_string()),
        );

    if let Some(log) = client_log {
        if let Ok(stderr) = log.try_clone() {
            command.stderr(stderr);
        }
        command.stdout(log);
    }

    match command.spawn()
    {
        Ok(_) => info!("Started startup Wayland client: {command_name}"),
        Err(err) => warn!("Failed to start startup Wayland client {command_name}: {err}"),
    }
}

fn send_frame_callbacks(state: &mut PancakeState) {
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
}
