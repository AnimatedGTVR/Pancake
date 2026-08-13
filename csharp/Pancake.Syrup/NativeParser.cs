using System.Globalization;

namespace Pancake.Syrup;

// Port of src/syrup/native.rs — native Syrup block syntax, plus the
// C-style (cpp/csharp) variant that just strips a leading type keyword.
public static class NativeParser
{
    public static SyrupDoc Parse(string src) => ParseInner(src, stripType: false);

    public static SyrupDoc ParseCStyle(string src) => ParseInner(src, stripType: true);

    private static SyrupDoc ParseInner(string src, bool stripType)
    {
        var doc = new SyrupDoc();
        string? currentSection = null;

        foreach (var rawLine in src.Split('\n'))
        {
            var line = StripComment(rawLine).Trim();
            if (line.Length == 0) continue;

            if (line.StartsWith("!lang")) continue;

            if (line.EndsWith('{'))
            {
                var name = line[..^1].Trim();
                currentSection = name;
                if (!doc.Sections.ContainsKey(name)) doc.Sections[name] = new SyrupSection();
                continue;
            }

            if (line == "}")
            {
                currentSection = null;
                continue;
            }

            if (currentSection is not null)
            {
                var assignment = line.TrimEnd(';').Trim();
                var split = SplitAssignment(assignment, stripType);
                if (split is not null)
                {
                    var (key, valueSrc) = split.Value;
                    var value = ParseValue(valueSrc);
                    if (value is not null)
                    {
                        if (!doc.Sections.TryGetValue(currentSection, out var section))
                            doc.Sections[currentSection] = section = new SyrupSection();
                        section[key] = value;
                    }
                }
            }
        }

        return doc;
    }

    private static (string Key, string Value)? SplitAssignment(string line, bool stripType)
    {
        var eq = line.IndexOf('=');
        if (eq < 0) return null;

        var keyPart = line[..eq].Trim();
        var valPart = line[(eq + 1)..].Trim();

        if (stripType)
        {
            var parts = keyPart.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length > 1) keyPart = parts[^1];
        }

        var keyTokens = keyPart.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);
        if (keyTokens.Length == 0) return null;

        return (keyTokens[0], valPart);
    }

    private static SyrupValue? ParseValue(string s)
    {
        s = s.Trim();

        if (s.StartsWith('[') && s.EndsWith(']'))
        {
            var inner = s[1..^1];
            var items = inner.Split(',')
                .Select(item => ParseScalar(item.Trim()))
                .Where(v => v is not null)
                .Select(v => v!)
                .ToList();
            return new SyrupValue.Array(items);
        }

        return ParseScalar(s);
    }

    private static SyrupValue? ParseScalar(string s)
    {
        s = s.Trim();
        if (s.Length == 0) return null;

        if ((s.StartsWith('"') && s.EndsWith('"') && s.Length >= 2) ||
            (s.StartsWith('\'') && s.EndsWith('\'') && s.Length >= 2))
        {
            return new SyrupValue.Str(s[1..^1]);
        }

        if (s == "true") return new SyrupValue.Bool(true);
        if (s == "false") return new SyrupValue.Bool(false);

        if (s.Contains('.') || s.Contains('e') || s.Contains('E'))
        {
            if (double.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture, out var f))
                return new SyrupValue.Float(f);
        }

        if (long.TryParse(s, NumberStyles.Integer, CultureInfo.InvariantCulture, out var i))
            return new SyrupValue.Int(i);

        return new SyrupValue.Str(s);
    }

    private static string StripComment(string line)
    {
        var inStr = false;
        for (var i = 0; i < line.Length; i++)
        {
            var c = line[i];
            if (c == '"')
            {
                inStr = !inStr;
            }
            else if (c == '#' && !inStr)
            {
                return line[..i];
            }
            else if (c == '/' && !inStr && i + 1 < line.Length && line[i + 1] == '/')
            {
                return line[..i];
            }
        }
        return line;
    }
}
