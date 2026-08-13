using System.Runtime.InteropServices;

namespace Pancake.Cn;

// P/Invoke bindings against libgbm.so -- the "Cn" surface for GPU buffer
// allocation. Deliberately minimal: only what Pancake.Cn.GpuDevice needs,
// not a full libgbm binding.
internal static unsafe partial class Gbm
{
    private const string Lib = "libgbm.so.1";

    [LibraryImport(Lib)]
    internal static partial nint gbm_create_device(int fd);

    [LibraryImport(Lib)]
    internal static partial void gbm_device_destroy(nint gbm);
}
