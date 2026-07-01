use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Client},
    wayland::compositor::{
        get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler, CompositorState,
    },
};

use crate::state::{ClientState, PancakeState};

impl CompositorHandler for PancakeState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        // Let smithay buffer-manage the surface before our logic.
        on_commit_buffer_handler::<Self>(surface);

        // Ignore synced subsurface commits — the parent commit handles damage.
        if is_sync_subsurface(surface) {
            return;
        }

        // Walk up to the root surface; damage/refresh only when fully committed.
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }

        // Refresh the space so the window's new buffer is shown on the next frame.
        self.space.refresh();
        self.popup_manager.cleanup();
    }
}
