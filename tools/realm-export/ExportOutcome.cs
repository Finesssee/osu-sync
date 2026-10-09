using System.Text.Json;

namespace RealmExport;

static class ExportOutcome
{
    /// Returns the failure message when every set was skipped, so a systemic read failure
    /// (schema change, Realm.dll mismatch) is not mistaken for an empty library.
    public static string Failure(int written, int skipped, string firstError) =>
        written == 0 && skipped > 0
            ? $"export failed: all {skipped} sets were skipped; first error: {firstError}"
            : null;
}

sealed class Skipped
{
    public int Count { get; private set; }
    public string First { get; private set; }

    public void Add(Exception e)
    {
        Count++;
        First ??= $"{e.GetType().Name}: {e.Message}";
    }

    public void WriteTo(Utf8JsonWriter w)
    {
        if (Count == 0)
        {
            w.WriteNull("skipped");
            return;
        }
        w.WriteStartObject("skipped");
        w.WriteNumber("count", Count);
        w.WriteString("first_error", First);
        w.WriteEndObject();
    }
}
