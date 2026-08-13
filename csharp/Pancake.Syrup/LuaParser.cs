using NLua;

namespace Pancake.Syrup;

// Port of src/syrup/lua.rs — Lua 5.4 frontend via NLua (the direct C#
// equivalent of the Rust mlua crate this was originally built on).
//
// The .confi file is executed as a Lua script. It must return a table
// whose keys are section names and whose values are tables of key=value
// pairs. Globals are sandboxed: os, io, require, load, loadfile, dofile,
// and package are removed so config scripts can't perform arbitrary
// system calls.
public static class LuaParser
{
    private static readonly string[] SandboxedGlobals =
        { "os", "io", "require", "load", "loadfile", "dofile", "package" };

    public static SyrupDoc Parse(string src)
    {
        var script = string.Join('\n',
            src.Split('\n').Where(l => !l.Trim().StartsWith("!lang")));

        try
        {
            return ParseLua(script);
        }
        catch (Exception)
        {
            // Matches the Rust side's "warn + return empty doc" behavior.
            return new SyrupDoc();
        }
    }

    private static SyrupDoc ParseLua(string script)
    {
        using var lua = new NLua.Lua();

        foreach (var key in SandboxedGlobals)
            lua[key] = null;

        var results = lua.DoString(script);
        var doc = new SyrupDoc();

        if (results.Length == 0 || results[0] is not LuaTable table) return doc;

        foreach (KeyValuePair<object, object> entry in table)
        {
            if (entry.Key is not string sectionName) continue;
            if (entry.Value is not LuaTable sectionTable) continue;

            var section = new SyrupSection();
            foreach (KeyValuePair<object, object> inner in sectionTable)
            {
                if (inner.Key is not string key) continue;
                var value = LuaValueToSyrup(inner.Value, lua);
                if (value is not null) section[key] = value;
            }
            doc.Sections[sectionName] = section;
        }

        return doc;
    }

    private static SyrupValue? LuaValueToSyrup(object? v, NLua.Lua lua)
    {
        switch (v)
        {
            case string s:
                return new SyrupValue.Str(s);
            case long l:
                return new SyrupValue.Int(l);
            case double d:
                // NLua returns whole numbers as double too; keep Rust's
                // Integer-vs-Number distinction as close as practical by
                // checking for an exact integral value.
                return d == Math.Floor(d) && !double.IsInfinity(d)
                    ? new SyrupValue.Int((long)d)
                    : new SyrupValue.Float(d);
            case bool b:
                return new SyrupValue.Bool(b);
            case LuaTable t:
                var arr = new List<SyrupValue>();
                foreach (KeyValuePair<object, object> pair in t)
                {
                    if (pair.Key is not long) { arr = null!; break; }
                    var sv = LuaValueToSyrup(pair.Value, lua);
                    if (sv is not null) arr.Add(sv);
                }
                return arr is { Count: > 0 } ? new SyrupValue.Array(arr) : null;
            default:
                return null;
        }
    }
}
