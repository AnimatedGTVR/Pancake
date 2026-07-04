use smithay::{
    desktop::{layer_map_for_output, LayerSurface as DesktopLayerSurface, WindowSurfaceType},
    reexports::wayland_server::protocol::wl_output::WlOutput,
    wayland::{
        shell::wlr_layer::{
            Layer, LayerSurface as WlrLayerSurface, WlrLayerShellHandler, WlrLayerShellState,
        },
    },
};
use tracing::{info, warn};

use crate::state::PancakeState;

impl WlrLayerShellHandler for PancakeState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        output: Option<WlOutput>,
        layer: Layer,
        namespace: String,
    ) {
        let output = output
            .as_ref()
            .and_then(smithay::output::Output::from_resource)
            .or_else(|| self.space.outputs().next().cloned());

        let Some(output) = output else {
            warn!("Layer surface '{namespace}' requested {layer:?}, but no output is available");
            return;
        };

        info!(
            "Mapping layer surface '{namespace}' on output '{}' as {layer:?}",
            output.name()
        );

        let layer_surface = DesktopLayerSurface::new(surface, namespace.clone());
        {
            let mut layer_map = layer_map_for_output(&output);
            if let Err(err) = layer_map.map_layer(&layer_surface) {
                warn!("Failed to map layer surface '{namespace}': {err}");
            }
        }
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        let wl_surface = surface.wl_surface();
        for output in self.space.outputs() {
            let mut layer_map = layer_map_for_output(output);
            let layer = layer_map
                .layer_for_surface(wl_surface, WindowSurfaceType::TOPLEVEL)
                .cloned();
            if let Some(layer) = layer {
                layer_map.unmap_layer(&layer);
            }
        }
    }
}
