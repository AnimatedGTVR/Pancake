using System.Text.Json;
using Pancake.Syrup;
using Tomlyn;

namespace Pancake.Config;

// Port of src/config.rs. Loads from (in order, first found wins):
//   $XDG_CONFIG_HOME/pancake/config.confi   -- Syrup universal format
//   $XDG_CONFIG_HOME/pancake/config.toml    -- legacy TOML (still supported)
public sealed class Keybinds
{
    public string Terminal = "Super+T";
    public string Close = "Super+Q";
    public string Quit = "Super+Escape";
    public string Cycle = "Super+Tab";
}

public sealed class PancakeConfig
{
    public string Terminal = "foot";
    public int BlurPasses = 4;
    public uint BlurDownsample = 2;
    public float[] Tint = { 0.55f, 0.70f, 1.00f, 0.18f };
    public string? Wallpaper;
    public Keybinds Keybinds = new();
    public List<string> StartupApps = new();

    public static PancakeConfig Load()
    {
        var baseDir = ConfigBaseDir();
        var confiPath = Path.Combine(baseDir, "config.confi");
        var tomlPath = Path.Combine(baseDir, "config.toml");

        if (File.Exists(confiPath))
        {
            var doc = Confi.Load(confiPath);
            return FromSyrup(doc);
        }

        if (File.Exists(tomlPath))
        {
            var fromToml = FromToml(tomlPath);
            if (fromToml is not null) return fromToml;
        }

        return Defaults();
    }

    private static PancakeConfig Defaults() => new()
    {
        Terminal = Environment.GetEnvironmentVariable("PANCAKE_TERMINAL") ?? "foot",
    };

    // -- Syrup loader --

    private static PancakeConfig FromSyrup(SyrupDoc doc)
    {
        var d = Defaults();

        var startupApps = new List<string>();
        if (doc.Get("startup", "apps") is SyrupValue.Array arr)
        {
            foreach (var v in arr.Values)
            {
                var s = v.AsStr();
                if (s is not null) startupApps.Add(s);
            }
        }

        return new PancakeConfig
        {
            Terminal = doc.StrVal("compositor", "terminal") ?? d.Terminal,
            BlurPasses = doc.IntVal("compositor", "blur_passes") is { } bp ? (int)bp : d.BlurPasses,
            BlurDownsample = doc.IntVal("compositor", "blur_downsample") is { } bd ? (uint)bd : d.BlurDownsample,
            Tint = doc.FloatArray("compositor", "tint", 4) ?? d.Tint,
            Wallpaper = doc.StrVal("compositor", "wallpaper"),
            Keybinds = new Keybinds
            {
                Terminal = doc.StrVal("keybinds", "terminal") ?? d.Keybinds.Terminal,
                Close = doc.StrVal("keybinds", "close") ?? d.Keybinds.Close,
                Quit = doc.StrVal("keybinds", "quit") ?? d.Keybinds.Quit,
                Cycle = doc.StrVal("keybinds", "cycle") ?? d.Keybinds.Cycle,
            },
            StartupApps = startupApps,
        };
    }

    // -- TOML loader (legacy) --

    private static readonly TomlSerializerOptions TomlOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    };

    private static PancakeConfig? FromToml(string path)
    {
        string src;
        try { src = File.ReadAllText(path); }
        catch { return null; }

        ConfigFile file;
        try { file = TomlSerializer.Deserialize<ConfigFile>(src, TomlOptions) ?? new ConfigFile(); }
        catch { return null; }

        var d = Defaults();
        var comp = file.Compositor;

        return new PancakeConfig
        {
            Terminal = comp?.Terminal ?? d.Terminal,
            BlurPasses = comp?.BlurPasses ?? d.BlurPasses,
            BlurDownsample = comp?.BlurDownsample ?? d.BlurDownsample,
            Tint = comp?.Tint is { Length: 4 } t ? t : d.Tint,
            Wallpaper = comp?.Wallpaper,
            Keybinds = new Keybinds(),
            StartupApps = new List<string>(),
        };
    }

    private sealed class ConfigFile
    {
        public CompositorSection? Compositor { get; set; }
    }

    private sealed class CompositorSection
    {
        public string? Terminal { get; set; }
        public int? BlurPasses { get; set; }
        public uint? BlurDownsample { get; set; }
        public float[]? Tint { get; set; }
        public string? Wallpaper { get; set; }
    }

    // -- Helpers --

    private static string ConfigBaseDir()
    {
        var baseDir = Environment.GetEnvironmentVariable("XDG_CONFIG_HOME");
        if (string.IsNullOrEmpty(baseDir))
        {
            var home = Environment.GetEnvironmentVariable("HOME") ?? "/root";
            baseDir = Path.Combine(home, ".config");
        }
        return Path.Combine(baseDir, "pancake");
    }
}
