using System.Runtime.InteropServices;

namespace Pancake.Cn;

public static partial class Libc
{
    private const string Lib = "libc";
    public const int O_RDWR = 2;
    public const int O_CLOEXEC = 0x80000;

    [LibraryImport(Lib, SetLastError = true, StringMarshalling = StringMarshalling.Utf8)]
    public static partial int open(string pathname, int flags);

    [LibraryImport(Lib, SetLastError = true)]
    public static partial int close(int fd);
}
