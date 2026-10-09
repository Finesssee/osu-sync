using System.Text;
using System.Text.Json;
using Xunit;

namespace RealmExport.Tests;

public class ExportOutcomeTests
{
    [Fact]
    public void Empty_library_is_not_a_failure() =>
        Assert.Null(ExportOutcome.Failure(0, 0, null));

    [Fact]
    public void Every_set_skipped_is_a_failure() =>
        Assert.Equal(
            "export failed: all 3 sets were skipped; first error: MissingMethodException: x",
            ExportOutcome.Failure(0, 3, "MissingMethodException: x"));

    [Fact]
    public void Some_sets_skipped_is_not_a_failure() =>
        Assert.Null(ExportOutcome.Failure(2, 1, "InvalidCastException: y"));

    [Fact]
    public void One_set_written_with_most_skipped_is_not_a_failure() =>
        Assert.Null(ExportOutcome.Failure(1, 7026, "MissingMethodException: z"));

    [Fact]
    public void Nothing_skipped_is_not_a_failure() =>
        Assert.Null(ExportOutcome.Failure(5, 0, null));

    [Fact]
    public void No_skips_write_null() =>
        Assert.Equal("{\"skipped\":null}", Envelope(new Skipped()));

    [Fact]
    public void Skips_write_the_count_and_first_error()
    {
        var skipped = new Skipped();
        skipped.Add(new InvalidCastException("first"));
        skipped.Add(new MissingMethodException("second"));

        Assert.Equal(
            "{\"skipped\":{\"count\":2,\"first_error\":\"InvalidCastException: first\"}}",
            Envelope(skipped));
    }

    static string Envelope(Skipped skipped)
    {
        using var stream = new MemoryStream();
        using (var w = new Utf8JsonWriter(stream))
        {
            w.WriteStartObject();
            skipped.WriteTo(w);
            w.WriteEndObject();
        }
        return Encoding.UTF8.GetString(stream.ToArray());
    }
}
