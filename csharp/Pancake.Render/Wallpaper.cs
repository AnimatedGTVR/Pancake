using SixLabors.ImageSharp;
using SixLabors.ImageSharp.PixelFormats;

namespace Pancake.Render;

// Port of src/render/aero.rs's load_wallpaper_rgba -- Rust's `image` crate
// becomes SixLabors.ImageSharp, the standard cross-platform C# equivalent.
internal static class Wallpaper
{
    internal static (byte[] Pixels, uint Width, uint Height) LoadRgba(string path)
    {
        using var img = Image.Load<Rgba32>(path);
        var pixels = new byte[img.Width * img.Height * 4];
        img.CopyPixelDataTo(pixels);
        return (pixels, (uint)img.Width, (uint)img.Height);
    }
}
