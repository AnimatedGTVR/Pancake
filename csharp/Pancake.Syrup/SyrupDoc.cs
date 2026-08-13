namespace Pancake.Syrup;

// Port of src/syrup/mod.rs's SyrupValue enum.
public abstract record SyrupValue
{
    public sealed record Str(string Value) : SyrupValue;
    public sealed record Int(long Value) : SyrupValue;
    public sealed record Float(double Value) : SyrupValue;
    public sealed record Bool(bool Value) : SyrupValue;
    public sealed record Array(IReadOnlyList<SyrupValue> Values) : SyrupValue;

    public string? AsStr() => this is Str s ? s.Value : null;

    public long? AsInt() => this switch
    {
        Int i => i.Value,
        Float f => (long)f.Value,
        _ => null,
    };

    public double? AsFloat() => this switch
    {
        Float f => f.Value,
        Int i => i.Value,
        _ => null,
    };

    public bool? AsBool() => this is Bool b ? b.Value : null;

    public float[]? AsFloatArray(int n)
    {
        if (this is not Array a || a.Values.Count != n) return null;
        var outArr = new float[n];
        for (var i = 0; i < n; i++)
        {
            var f = a.Values[i].AsFloat();
            if (f is null) return null;
            outArr[i] = (float)f.Value;
        }
        return outArr;
    }
}

// A section -> (key -> value) map. Flat sections like `compositor.terminal`.
public sealed class SyrupSection : Dictionary<string, SyrupValue> { }

// Top-level document: section name -> its key/value pairs.
public sealed class SyrupDoc
{
    public Dictionary<string, SyrupSection> Sections { get; } = new();

    public SyrupValue? Get(string section, string key) =>
        Sections.TryGetValue(section, out var s) && s.TryGetValue(key, out var v) ? v : null;

    public string? StrVal(string section, string key) => Get(section, key)?.AsStr();
    public long? IntVal(string section, string key) => Get(section, key)?.AsInt();
    public double? FloatVal(string section, string key) => Get(section, key)?.AsFloat();
    public bool? BoolVal(string section, string key) => Get(section, key)?.AsBool();
    public float[]? FloatArray(string section, string key, int n) => Get(section, key)?.AsFloatArray(n);
}
