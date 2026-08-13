namespace Pancake.Shell;

// Port of the slice of Smithay's `output::Output` that Space actually
// needs: a name and a geometry. Smithay's real Output carries physical
// properties/modes/scale/transform too, but nothing in the ported code
// so far (borders.rs, layout.rs, gpu.rs's output enumeration) reads
// those through Space -- they belong with the eventual DRM/KMS output
// enumeration (Pancake.Cn's DrmResources), not here.
public sealed class PancakeOutput
{
    public required string Name { get; init; }
}
