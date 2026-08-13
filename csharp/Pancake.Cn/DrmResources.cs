namespace Pancake.Cn;

public sealed record DrmConnectorInfo(uint Id, uint Type, uint TypeId, bool Connected);

public sealed record DrmResourcesInfo(int CrtcCount, int ConnectorCount, int EncoderCount, IReadOnlyList<DrmConnectorInfo> Connectors);

// Port of the connector/CRTC enumeration slice of GpuData::init in
// src/backend/gpu.rs (drm.resource_handles() / drm.get_connector()).
// Deliberately read-only: this needs no DRM_MASTER (unlike an actual
// modeset/atomic-commit, which does), so it's usable for diagnostics
// even on a "card" node this process doesn't have master on.
public static class DrmResources
{
    public static unsafe DrmResourcesInfo Query(int fd)
    {
        var resPtr = Drm.drmModeGetResources(fd);
        if (resPtr == 0) throw new InvalidOperationException("drmModeGetResources failed");

        try
        {
            var res = *(DrmModeRes*)resPtr;
            var connectors = new List<DrmConnectorInfo>();

            var connectorIds = (uint*)res.connectors;
            for (var i = 0; i < res.count_connectors; i++)
            {
                var connId = connectorIds[i];
                var connPtr = Drm.drmModeGetConnector(fd, connId);
                if (connPtr == 0) continue;
                try
                {
                    var conn = *(DrmModeConnector*)connPtr;
                    connectors.Add(new DrmConnectorInfo(
                        conn.connector_id,
                        conn.connector_type,
                        conn.connector_type_id,
                        conn.connection == Drm.DRM_MODE_CONNECTED));
                }
                finally
                {
                    Drm.drmModeFreeConnector(connPtr);
                }
            }

            return new DrmResourcesInfo(res.count_crtcs, res.count_connectors, res.count_encoders, connectors);
        }
        finally
        {
            Drm.drmModeFreeResources(resPtr);
        }
    }
}
