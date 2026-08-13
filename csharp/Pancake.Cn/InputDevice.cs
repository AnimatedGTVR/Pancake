namespace Pancake.Cn;

// Real libinput bring-up against a real evdev device node: create a
// context, add the device, and read back the DEVICE_ADDED event libinput
// always queues synchronously on a successful add -- no physical input
// activity needed to prove the pipeline is real, unlike pointer/keyboard
// events which do need someone to actually move a mouse.
public sealed class InputDevice : IDisposable
{
    private nint _libinput;

    public bool DeviceAddedEventReceived { get; private set; }

    public static unsafe InputDevice Open(string evdevPath)
    {
        var dev = new InputDevice();
        try
        {
            var iface = new Libinput.LibinputInterface
            {
                OpenRestricted = (nint)(delegate* unmanaged[Cdecl]<byte*, int, void*, int>)&Libinput.OpenRestricted,
                CloseRestricted = (nint)(delegate* unmanaged[Cdecl]<int, void*, void>)&Libinput.CloseRestricted,
            };

            dev._libinput = Libinput.libinput_path_create_context(in iface, 0);
            if (dev._libinput == 0)
                throw new InvalidOperationException("libinput_path_create_context failed");

            var device = Libinput.libinput_path_add_device(dev._libinput, evdevPath);
            if (device == 0)
                throw new InvalidOperationException($"libinput_path_add_device({evdevPath}) failed");

            Libinput.libinput_dispatch(dev._libinput);

            nint evt;
            while ((evt = Libinput.libinput_get_event(dev._libinput)) != 0)
            {
                var type = Libinput.libinput_event_get_type(evt);
                if (type == Libinput.LIBINPUT_EVENT_DEVICE_ADDED)
                    dev.DeviceAddedEventReceived = true;
                Libinput.libinput_event_destroy(evt);
            }

            return dev;
        }
        catch
        {
            dev.Dispose();
            throw;
        }
    }

    public void Dispose()
    {
        if (_libinput != 0)
        {
            Libinput.libinput_unref(_libinput);
            _libinput = 0;
        }
    }
}
