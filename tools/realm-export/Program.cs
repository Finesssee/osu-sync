using System.Runtime.InteropServices;
using System.Runtime.Loader;

namespace RealmExport;

static class Program
{
    const string Usage =
        "usage:\n" +
        "  realm-export export <client.realm> [--out <file>] [--lazer-dir <dir>]\n" +
        "  realm-export trim <client.realm> --keep <ids-file> [--lazer-dir <dir>]\n" +
        "  realm-export mark-delete-pending <client.realm> --id <set-id> [--lazer-dir <dir>]\n" +
        "--lazer-dir is the osu!lazer install folder holding Realm.dll, by default %LOCALAPPDATA%\\osulazer\\current.\n" +
        "trim and mark-delete-pending only accept a realm under " + Sandbox.Root + ".";

    static int Main(string[] args)
    {
        try
        {
            return Run(args);
        }
        catch (ToolException e)
        {
            Console.Error.WriteLine(e.Message);
            return e.ExitCode;
        }
    }

    static int Run(string[] args)
    {
        if (args.Length < 2)
            throw new ToolException(2, Usage);

        string command = args[0];
        string realmPath = Path.GetFullPath(args[1]);
        var options = ParseOptions(args.Skip(2).ToArray());

        if (!File.Exists(realmPath))
            throw new ToolException(1, $"client.realm not found at {realmPath}");

        string lazerDir = options.GetValueOrDefault("--lazer-dir") ?? DefaultLazerDir();
        LoadLazerRealm(lazerDir);

        switch (command)
        {
            case "export":
                Commands.Export(realmPath, options.GetValueOrDefault("--out"));
                return 0;
            case "trim":
                Sandbox.Require(realmPath);
                Commands.Trim(realmPath, Required(options, "--keep"));
                return 0;
            case "mark-delete-pending":
                Sandbox.Require(realmPath);
                Commands.MarkDeletePending(realmPath, Required(options, "--id"));
                return 0;
            default:
                throw new ToolException(2, $"unknown command {command}\n{Usage}");
        }
    }

    static Dictionary<string, string> ParseOptions(string[] args)
    {
        var options = new Dictionary<string, string>();
        for (int i = 0; i < args.Length; i += 2)
        {
            if (!args[i].StartsWith("--") || i + 1 >= args.Length)
                throw new ToolException(2, $"unexpected argument {args[i]}\n{Usage}");
            options[args[i]] = args[i + 1];
        }
        return options;
    }

    static string Required(Dictionary<string, string> options, string name) =>
        options.GetValueOrDefault(name) ?? throw new ToolException(2, $"missing {name}\n{Usage}");

    static string DefaultLazerDir()
    {
        string localAppData = Environment.GetEnvironmentVariable("LOCALAPPDATA") ?? "";
        return Path.Combine(localAppData, "osulazer", "current");
    }

    static void LoadLazerRealm(string lazerDir)
    {
        lazerDir = Path.GetFullPath(lazerDir);
        foreach (string name in new[] { "Realm.dll", "MongoDB.Bson.dll", "Remotion.Linq.dll", "realm-wrappers.dll" })
        {
            if (!File.Exists(Path.Combine(lazerDir, name)))
                throw new ToolException(3,
                    $"{name} not found in {lazerDir}. Install osu!lazer or pass --lazer-dir with the folder that holds Realm.dll.");
        }

        NativeLibrary.Load(Path.Combine(lazerDir, "realm-wrappers.dll"));
        AssemblyLoadContext.Default.Resolving += (context, name) =>
        {
            string candidate = Path.Combine(lazerDir, name.Name + ".dll");
            return File.Exists(candidate) ? context.LoadFromAssemblyPath(candidate) : null;
        };
    }
}

sealed class ToolException(int exitCode, string message) : Exception(message)
{
    public int ExitCode { get; } = exitCode;
}

static class Sandbox
{
    public const string Root = @"D:\osu-sync-sandbox";

    public static void Require(string path)
    {
        string full = Path.GetFullPath(path);
        string[] parts = full.TrimEnd('\\', '/').Split('\\', '/');
        string[] root = Root.Split('\\');
        bool inside = parts.Length > root.Length
            && root.Select((part, i) => string.Equals(part, parts[i], StringComparison.OrdinalIgnoreCase)).All(x => x);
        if (!inside)
            throw new ToolException(4, $"Refusing to write {full} because it is outside {Root}");

        for (string dir = Path.GetDirectoryName(full); dir != null; dir = Path.GetDirectoryName(dir))
        {
            if (Directory.Exists(dir) && new DirectoryInfo(dir).Attributes.HasFlag(FileAttributes.ReparsePoint))
                throw new ToolException(4, $"Refusing to write {full} because {dir} is a junction or symbolic link");
        }
        if (new FileInfo(full).Attributes.HasFlag(FileAttributes.ReparsePoint))
            throw new ToolException(4, $"Refusing to write {full} because it is a symbolic link");
    }
}
