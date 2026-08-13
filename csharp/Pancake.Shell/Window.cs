namespace Pancake.Shell;

// Port of Smithay's Window identity semantics: `desktop::Window` is a
// cheap-clone, Rc-backed handle compared by pointer identity (`PartialEq`
// on `Window` is identity, not structural, across the whole codebase --
// every `w == window` in workspace.rs/layout.rs/xdg_shell.rs relies on
// this). A plain reference-equality class is the direct C# equivalent;
// no need for Rc/clone machinery since C# references already work this
// way.
public sealed class PancakeWindow : IEquatable<PancakeWindow>
{
    // Opaque payload the Wayland layer attaches (its WlSurface.Server /
    // XdgToplevel.Server pair) -- PancakeShell doesn't need to know what
    // this is, only track it, same as Smithay's Window not caring what
    // handlers do with the surface it wraps.
    public object? Backend { get; set; }

    public string? Title { get; set; }
    public string? AppId { get; set; }

    public bool Equals(PancakeWindow? other) => ReferenceEquals(this, other);
    public override bool Equals(object? obj) => Equals(obj as PancakeWindow);
    public override int GetHashCode() => System.Runtime.CompilerServices.RuntimeHelpers.GetHashCode(this);
}
