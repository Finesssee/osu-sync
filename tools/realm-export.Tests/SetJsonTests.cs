using System.Buffers;
using System.Text;
using System.Text.Json;
using Xunit;

namespace RealmExport.Tests;

public class SetJsonTests
{
    static string Write(SetRecord set)
    {
        var buffer = new ArrayBufferWriter<byte>();
        using (var w = new Utf8JsonWriter(buffer))
            SetJson.WriteSet(w, set);
        return Encoding.UTF8.GetString(buffer.WrittenSpan);
    }

    [Fact]
    public void Date_added_is_written_as_utc_iso_8601()
    {
        var set = new SetRecord(
            "6f0a1c2e-0000-4000-8000-000000000001", 1001, false, false,
            new DateTimeOffset(2024, 3, 5, 14, 7, 9, 250, TimeSpan.FromHours(7)),
            "Artist", "Title", "Mapper",
            new List<BeatmapRecord>(),
            new List<FileRecord> { new("audio.mp3", "ab12") });

        Assert.Equal(
            "{\"id\":\"6f0a1c2e-0000-4000-8000-000000000001\",\"online_id\":1001,\"protected\":false,"
            + "\"delete_pending\":false,\"date_added\":\"2024-03-05T07:07:09.25+00:00\","
            + "\"artist\":\"Artist\",\"title\":\"Title\",\"creator\":\"Mapper\",\"beatmaps\":[],"
            + "\"files\":[{\"filename\":\"audio.mp3\",\"hash\":\"ab12\"}]}",
            Write(set));
    }
}
