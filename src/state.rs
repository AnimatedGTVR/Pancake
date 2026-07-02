use std::os::unix::io::OwnedFd;

use smithay::{
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm,
    delegate_xdg_shell,
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState},
    reexports::wayland_server::{
        backend::{ClientData, ClientId, DisconnectReason},
        protocol::wl_buffer,
        Display, DisplayHandle,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorState},
        output::{OutputHandler, OutputManagerState},
        selection::{
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
            SelectionHandler,
        },
        shell::xdg::XdgShellState,
        shm::{ShmHandler, ShmState},
    },
    xwayland::xwm::X11Wm,
};

use crate::config::{Config, RELOAD_REQUESTED};
use crate::render::AeroRenderer;

// ── Per-client state ────────────────────────────────────────────────────────

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _id: ClientId) {}
    fn disconnected(&self, _id: ClientId, _reason: DisconnectReason) {}
}

// ── Compositor-wide state ───────────────────────────────────────────────────

#[allow(dead_code)]
pub struct PancakeState {
    pub display_handle: DisplayHandle,

    // Core Wayland protocols
    pub compositor_state: CompositorState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,

    // XDG shell (application windows)
    pub xdg_shell_state: XdgShellState,
    pub popup_manager: PopupManager,

    // Input
    pub seat_state: SeatState<Self>,
    pub seat: Seat<Self>,

    // Clipboard + drag-and-drop
    pub data_device_state: DataDeviceState,

    // Layout space — windows live here
    pub space: Space<Window>,

    // XWayland window manager (started lazily)
    pub xwm: Option<X11Wm>,

    // Aero glass rendering pipeline
    pub renderer: AeroRenderer,

    // Runtime configuration (terminal command, etc.)
    pub config: Config,

    // Currently focused window, tracked for Super+Tab cycling.
    pub focused_window: Option<Window>,
}

impl PancakeState {
    pub fn new(display: &Display<Self>) -> Self {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&dh, "pancake");
        seat.add_keyboard(Default::default(), 200, 25)
            .expect("keyboard init failed");
        seat.add_pointer();

        let config = Config::load();
        let mut renderer = AeroRenderer::default();
        renderer.apply_config(&config);

        Self {
            display_handle: dh,
            compositor_state,
            shm_state,
            output_manager_state,
            xdg_shell_state,
            popup_manager: PopupManager::default(),
            seat_state,
            seat,
            data_device_state,
            space: Space::default(),
            xwm: None,
            renderer,
            config,
            focused_window: None,
        }
    }

    /// Reload config from disk and apply any changed values.
    pub fn reload_config(&mut self) {
        use std::sync::atomic::Ordering;
        RELOAD_REQUESTED.store(false, Ordering::Relaxed);
        self.config = Config::load();
        self.renderer.apply_config(&self.config);
        tracing::info!("Config reloaded");
    }

    /// Check if a SIGHUP-triggered reload is pending and, if so, reload.
    pub fn maybe_reload_config(&mut self) {
        use std::sync::atomic::Ordering;
        if RELOAD_REQUESTED.load(Ordering::Relaxed) {
            self.reload_config();
        }
    }

    /// Cycle keyboard focus to the next window in the space.
    ///
    /// Windows are cycled in map order. The focused window is raised to the top
    /// so it is visually on top of everything else.
    pub fn cycle_focus(&mut self, serial: smithay::utils::Serial) {
        let windows: Vec<Window> = self.space.elements().cloned().collect();
        if windows.is_empty() {
            return;
        }

        let next = if let Some(cur) = &self.focused_window {
            let pos = windows.iter().position(|w| w == cur);
            match pos {
                Some(i) => windows[(i + 1) % windows.len()].clone(),
                None => windows[0].clone(),
            }
        } else {
            windows[0].clone()
        };

        self.space.raise_element(&next, true);

        use smithay::wayland::seat::WaylandFocus;
        if let Some(surf) = next.wl_surface() {
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, Some(surf.into_owned()), serial);
            }
        }

        self.focused_window = Some(next);
    }
}

// ── Required trait impls (many are satisfying compiler constraints for delegate!) ──

impl BufferHandler for PancakeState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for PancakeState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl OutputHandler for PancakeState {}

impl SelectionHandler for PancakeState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for PancakeState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for PancakeState {}

impl ServerDndGrabHandler for PancakeState {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

// ── Protocol delegation macros ───────────────────────────────────────────────

delegate_compositor!(PancakeState);
delegate_xdg_shell!(PancakeState);
delegate_shm!(PancakeState);
delegate_output!(PancakeState);
delegate_seat!(PancakeState);
delegate_data_device!(PancakeState);
