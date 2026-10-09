using System.Text.Json;

namespace RealmExport;

/// Realm-free JSON shape of one exported set, so the tests can compile it without Realm.dll.
static class SetJson
{
    static void WriteNumberOrNull(Utf8JsonWriter w, string name, double value)
    {
        if (double.IsFinite(value))
            w.WriteNumber(name, value);
        else
            w.WriteNull(name);
    }

    static void WriteNumberOrNull(Utf8JsonWriter w, string name, float value)
    {
        if (float.IsFinite(value))
            w.WriteNumber(name, value);
        else
            w.WriteNull(name);
    }

    public static void WriteSet(Utf8JsonWriter w, SetRecord set)
    {
        w.WriteStartObject();
        w.WriteString("id", set.Id);
        w.WriteNumber("online_id", set.OnlineId);
        w.WriteBoolean("protected", set.Protected);
        w.WriteBoolean("delete_pending", set.DeletePending);
        w.WriteString("date_added", set.DateAdded.ToUniversalTime());
        w.WriteString("artist", set.Artist);
        w.WriteString("title", set.Title);
        w.WriteString("creator", set.Creator);
        w.WriteStartArray("beatmaps");
        foreach (var b in set.Beatmaps)
        {
            w.WriteStartObject();
            w.WriteString("id", b.Id);
            w.WriteNumber("online_id", b.OnlineId);
            w.WriteString("hash", b.Hash);
            w.WriteString("md5_hash", b.Md5Hash);
            w.WriteString("difficulty_name", b.DifficultyName);
            w.WriteNumber("ruleset", b.Ruleset);
            WriteNumberOrNull(w, "length_ms", b.LengthMs);
            WriteNumberOrNull(w, "bpm", b.Bpm);
            WriteNumberOrNull(w, "star_rating", b.StarRating);
            w.WriteNumber("status", b.Status);
            w.WriteBoolean("hidden", b.Hidden);
            w.WriteString("title", b.Title);
            w.WriteString("title_unicode", b.TitleUnicode);
            w.WriteString("artist", b.Artist);
            w.WriteString("artist_unicode", b.ArtistUnicode);
            w.WriteString("author", b.Author);
            w.WriteString("source", b.Source);
            w.WriteString("tags", b.Tags);
            WriteNumberOrNull(w, "drain_rate", b.DrainRate);
            WriteNumberOrNull(w, "circle_size", b.CircleSize);
            WriteNumberOrNull(w, "overall_difficulty", b.OverallDifficulty);
            WriteNumberOrNull(w, "approach_rate", b.ApproachRate);
            WriteNumberOrNull(w, "slider_multiplier", b.SliderMultiplier);
            WriteNumberOrNull(w, "slider_tick_rate", b.SliderTickRate);
            w.WriteEndObject();
        }
        w.WriteEndArray();
        w.WriteStartArray("files");
        foreach (var f in set.Files)
        {
            w.WriteStartObject();
            w.WriteString("filename", f.Filename);
            w.WriteString("hash", f.Hash);
            w.WriteEndObject();
        }
        w.WriteEndArray();
        w.WriteEndObject();
    }
}

record SetRecord(
    string Id, int OnlineId, bool Protected, bool DeletePending, DateTimeOffset DateAdded,
    string Artist, string Title, string Creator,
    List<BeatmapRecord> Beatmaps, List<FileRecord> Files);

record BeatmapRecord(
    string Id, int OnlineId, string Hash, string Md5Hash, string DifficultyName, int Ruleset,
    double LengthMs, double Bpm, double StarRating, int Status, bool Hidden,
    string Title, string TitleUnicode, string Artist, string ArtistUnicode, string Author,
    string Source, string Tags,
    float DrainRate, float CircleSize, float OverallDifficulty, float ApproachRate,
    double SliderMultiplier, double SliderTickRate);

record FileRecord(string Filename, string Hash);
