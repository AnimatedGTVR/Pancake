use smithay::{
    desktop::{PopupKind, Window},
    reexports::wayland_server::protocol::wl_seat::WlSeat,
    utils::Serial,
    wayland::{
        seat::WaylandFocus,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        },
    },
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;

use crate::shell::layout;
use crate::state::PancakeState;

impl XdgShellHandler for PancakeState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    // ── Toplevels (regular application windows) ─────────────────────────────

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_configure();

        let window = Window::new_wayland_window(surface);
        let pos = layout::initial_position(&self.space);
        self.space.map_element(window, pos, true);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let target = surface.wl_surface().clone();
        let window = self
            .space
            .elements()
            .find(|w| w.wl_surface().as_deref() == Some(&target))
            .cloned();
        if let Some(w) = window {
            self.space.unmap_elem(&w);
        }
    }

    fn move_request(&mut self, _surface: ToplevelSurface, _seat: WlSeat, _serial: Serial) {
        // TODO: interactive move grab
    }

    fn resize_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: WlSeat,
        _serial: Serial,
        _edges: xdg_toplevel::ResizeEdge,
    ) {
        // TODO: interactive resize grab
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(output) = self.space.outputs().next() {
            let geo = self.space.output_geometry(output).unwrap_or_default();
            surface.with_pending_state(|s| {
                s.states.set(xdg_toplevel::State::Maximized);
                s.size = Some(geo.size);
            });
        }
        surface.send_pending_configure();
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Maximized);
            s.size = None;
        });
        surface.send_pending_configure();
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
    ) {
        surface.with_pending_state(|s| {
            s.states.set(xdg_toplevel::State::Fullscreen);
        });
        surface.send_pending_configure();
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Fullscreen);
            s.size = None;
        });
        surface.send_pending_configure();
    }

    // ── Popups (menus, tooltips, dropdowns) ─────────────────────────────────

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        if let Err(e) = self.popup_manager.track_popup(PopupKind::Xdg(surface)) {
            tracing::warn!("Failed to track popup: {e}");
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|s| {
            s.geometry = positioner.get_geometry();
        });
        surface.send_repositioned(token);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {
        // TODO: exclusive popup grab
    }
}
