use std::sync::atomic::{AtomicUsize, Ordering};

use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    desktop::layer_map_for_output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Client, Resource},
    wayland::compositor::{
        get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler, CompositorState,
    },
};

use crate::state::{ClientState, PancakeState};

static COMMIT_LOG_COUNT: AtomicUsize = AtomicUsize::new(0);

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

        // Re-arrange layer surfaces on every commit so size/position changes
        // (e.g. waybar changing its height) are reflected immediately.
        for output in self.space.outputs().cloned().collect::<Vec<_>>() {
            let mut layer_map = layer_map_for_output(&output);
            layer_map.arrange();
        }

        // Refresh the space so the window's new buffer is shown on the next frame.
        self.space.refresh();
        self.popup_manager.cleanup();

        let n = COMMIT_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
        if n < 24 {
            tracing::info!(
                "Committed Wayland surface; root={:?}, space_windows={}, xdg_toplevels={}",
                root.id(),
                self.space.elements().count(),
                self.xdg_shell_state.toplevel_surfaces().len()
            );
        }
    }
}
