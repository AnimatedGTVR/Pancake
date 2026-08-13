namespace Pancake.Render;

internal sealed record XcursorImage(uint Size, uint Width, uint Height, uint XHot, uint YHot, uint[] PixelsArgb);

// Hand-rolled parser for the Xcursor binary file format -- Rust's
// `xcursor` crate becomes this. The format is small and stable enough
// (X.Org's XCURSOR spec, unchanged in decades) that hand-rolling it here
// keeps the same "read the real bytes, no external dependency" approach
// as RawWaylandClient's wire-protocol parsing earlier this session.
// Verified against a real file (/usr/share/icons/Adwaita/cursors/default)
// read with a Python struct-based reference parse before writing this,
// same as double-checking the Wayland wire format against real traffic.
internal static class XcursorFile
{
    private const uint ImageChunkType = 0xfffd0002;

    public static List<XcursorImage>? Parse(byte[] data)
    {
        if (data.Length < 16) return null;
        if (data[0] != 'X' || data[1] != 'c' || data[2] != 'u' || data[3] != 'r') return null;

        var headerSize = ReadU32(data, 4);
        // version at offset 8, unused here.
        var ntoc = ReadU32(data, 12);

        var images = new List<XcursorImage>();
        var tocOffset = headerSize;

        for (var i = 0; i < ntoc; i++)
        {
            var entryOffset = tocOffset + (uint)i * 12;
            if (entryOffset + 12 > data.Length) break;

            var type = ReadU32(data, entryOffset);
            var subtype = ReadU32(data, entryOffset + 4);
            var position = ReadU32(data, entryOffset + 8);

            if (type != ImageChunkType) continue;
            if (position + 36 > data.Length) continue;

            // Chunk header: chunk_header_size, type, subtype, version (16 bytes),
            // then width, height, xhot, yhot, delay (20 bytes) = 36 total.
            var width = ReadU32(data, position + 16);
            var height = ReadU32(data, position + 20);
            var xhot = ReadU32(data, position + 24);
            var yhot = ReadU32(data, position + 28);

            var pixelsOffset = position + 36;
            var pixelCount = width * height;
            if (pixelsOffset + pixelCount * 4 > data.Length) continue;

            var pixels = new uint[pixelCount];
            for (var p = 0; p < pixelCount; p++)
                pixels[p] = ReadU32(data, pixelsOffset + (uint)p * 4);

            images.Add(new XcursorImage(subtype, width, height, xhot, yhot, pixels));
        }

        return images;
    }

    private static uint ReadU32(byte[] data, uint offset) =>
        (uint)(data[offset] | (data[offset + 1] << 8) | (data[offset + 2] << 16) | (data[offset + 3] << 24));
}
