using System.Runtime.InteropServices;

namespace Pancake.Config;

// Port of src/config.rs's SIGHUP reload flag. The Rust side needed a raw
// `libc::signal` FFI call (its one bit of `unsafe` in this file); .NET's
// PosixSignalRegistration covers SIGHUP natively on Linux, so this needs
// no Cn at all -- one fewer file in the Cn surface than the original
// "3 files use unsafe" guess assumed.
public static class ReloadSignal
{
    private static volatile bool _reloadRequested;
    private static PosixSignalRegistration? _registration;

    public static bool ReloadRequested
    {
        get => _reloadRequested;
        set => _reloadRequested = value;
    }

    public static void Install()
    {
        _registration = PosixSignalRegistration.Create(PosixSignal.SIGHUP, ctx =>
        {
            _reloadRequested = true;
            ctx.Cancel = true;
        });
    }
}
