namespace Pancake.Syrup;

// Port of src/syrup/mod.rs — language detection + dispatch for .confi files.
public static class Confi
{
    private enum ConfiLang
    {
        Syrup,
        Lua,
        Cpp, // C++ style -- same parser as C#
        CSharp,
    }

    private static ConfiLang DetectLang(string src)
    {
        foreach (var rawLine in src.Split('\n'))
        {
            var trimmed = rawLine.Trim();
            if (trimmed.Length == 0 || trimmed.StartsWith('#') || trimmed.StartsWith("//"))
                continue;

            if (trimmed.StartsWith("!lang"))
            {
                var lang = trimmed["!lang".Length..].Trim().ToLowerInvariant();
                return lang switch
                {
                    "lua" or "luajit" => ConfiLang.Lua,
                    "cpp" or "c++" => ConfiLang.Cpp,
                    "csharp" or "c#" => ConfiLang.CSharp,
                    _ => ConfiLang.Syrup,
                };
            }

            // No !lang directive on the first meaningful line -> native.
            break;
        }
        return ConfiLang.Syrup;
    }

    public static SyrupDoc Parse(string src) => DetectLang(src) switch
    {
        ConfiLang.Lua => LuaParser.Parse(src),
        ConfiLang.Cpp or ConfiLang.CSharp => NativeParser.ParseCStyle(src),
        ConfiLang.Syrup => NativeParser.Parse(src),
        _ => NativeParser.Parse(src),
    };

    public static SyrupDoc Load(string path)
    {
        try
        {
            return Parse(File.ReadAllText(path));
        }
        catch (FileNotFoundException)
        {
            return new SyrupDoc();
        }
        catch (DirectoryNotFoundException)
        {
            return new SyrupDoc();
        }
        catch (Exception)
        {
            return new SyrupDoc();
        }
    }
}
