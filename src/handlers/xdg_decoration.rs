use smithay::wayland::shell::xdg::{
    decoration::XdgDecorationHandler,
    ToplevelSurface,
};
use wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;

use crate::state::PancakeState;

impl XdgDecorationHandler for PancakeState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        // Tell every client to use client-side decorations — Pancake doesn't
        // draw server-side title bars yet. Apps that honour this (GTK4, Qt6)
        // will draw their own CSD; ones that ignore it behave as before.
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ClientSide);
        });
        toplevel.send_pending_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        let granted = match mode {
            Mode::ServerSide => Mode::ClientSide,
            _ => mode,
        };
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(granted);
        });
        toplevel.send_pending_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = None;
        });
        toplevel.send_pending_configure();
    }

}
