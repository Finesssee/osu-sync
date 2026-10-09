using System.Text.Json;
using Realms;

namespace RealmExport;

static class Commands
{
    public static void Export(string realmPath, string outPath)
    {
        string tempDir = Path.Combine(Path.GetTempPath(), "osu-sync-realm-export-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            string copy = Path.Combine(tempDir, "client.realm");
            using (var source = new FileStream(realmPath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
            using (var target = File.Create(copy))
                source.CopyTo(target);

            List<SetRecord> sets;
            using (var realm = Realm.GetInstance(new RealmConfiguration(copy) { IsDynamic = true, IsReadOnly = true }))
                sets = ReadSets(realm);

            using Stream output = outPath == null ? Console.OpenStandardOutput() : File.Create(outPath);
            Write(sets, output);
            Console.Error.WriteLine($"exported {sets.Count} sets, {sets.Count(s => s.DeletePending)} delete-pending");
        }
        finally
        {
            Directory.Delete(tempDir, true);
        }
    }

    public static void Trim(string realmPath, string keepFile)
    {
        var keep = File.ReadAllLines(keepFile)
            .Select(line => line.Trim())
            .Where(line => line.Length > 0)
            .Select(Guid.Parse)
            .ToHashSet();

        var config = new RealmConfiguration(realmPath) { IsDynamic = true };
        int removed = 0, kept = 0;
        using (var realm = Realm.GetInstance(config))
        {
            var doomed = realm.DynamicApi.All("BeatmapSet").ToList()
                .Where(set => !keep.Contains(set.DynamicApi.Get<Guid>("ID")))
                .ToList();
            kept = realm.DynamicApi.All("BeatmapSet").Count() - doomed.Count;
            realm.Write(() =>
            {
                foreach (var set in doomed)
                {
                    RemoveSet(realm, set);
                    removed++;
                }
            });
        }
        Realm.Compact(config);
        Console.Error.WriteLine($"trimmed {realmPath}: kept {kept} sets, removed {removed}");
        if (kept != keep.Count)
            throw new ToolException(1, $"{keep.Count} ids requested but {kept} sets kept");
    }

    public static void MarkDeletePending(string realmPath, string setId)
    {
        var id = Guid.Parse(setId);
        using var realm = Realm.GetInstance(new RealmConfiguration(realmPath) { IsDynamic = true });
        var set = realm.DynamicApi.All("BeatmapSet").ToList()
            .FirstOrDefault(s => s.DynamicApi.Get<Guid>("ID") == id)
            ?? throw new ToolException(1, $"no set with id {id} in {realmPath}");
        realm.Write(() => set.DynamicApi.Set("DeletePending", true));
        Console.Error.WriteLine($"marked {id} delete-pending in {realmPath}");
    }

    static void RemoveSet(Realm realm, IRealmObject set)
    {
        foreach (var beatmap in set.DynamicApi.GetList<IRealmObject>("Beatmaps").ToList())
        {
            foreach (var score in beatmap.DynamicApi.GetBacklinksFromType("Score", "BeatmapInfo").ToList())
                realm.Remove(score);
            var metadata = beatmap.DynamicApi.Get<IRealmObject>("Metadata");
            if (metadata != null && metadata.IsValid)
                realm.Remove(metadata);
            realm.Remove(beatmap);
        }
        realm.Remove(set);
    }

    static List<SetRecord> ReadSets(Realm realm)
    {
        var sets = new List<SetRecord>();
        foreach (var set in realm.DynamicApi.All("BeatmapSet"))
        {
            var api = set.DynamicApi;
            var beatmaps = api.GetList<IRealmObject>("Beatmaps").Select(ReadBeatmap).ToList();
            beatmaps.Sort((a, b) => a.OnlineId != b.OnlineId
                ? a.OnlineId.CompareTo(b.OnlineId)
                : string.CompareOrdinal(a.Id, b.Id));

            var files = api.GetList<IEmbeddedObject>("Files").Select(usage =>
            {
                var file = usage.DynamicApi.Get<IRealmObject>("File");
                return new FileRecord(usage.DynamicApi.Get<string>("Filename"), file?.DynamicApi.Get<string>("Hash"));
            }).ToList();
            files.Sort((a, b) => string.CompareOrdinal(a.Filename, b.Filename) is var c && c != 0
                ? c
                : string.CompareOrdinal(a.Hash, b.Hash));

            var first = beatmaps.Count > 0 ? beatmaps[0] : null;
            sets.Add(new SetRecord(
                api.Get<Guid>("ID").ToString(),
                api.Get<int>("OnlineID"),
                api.Get<bool>("Protected"),
                api.Get<bool>("DeletePending"),
                first?.Artist,
                first?.Title,
                first?.Author,
                beatmaps,
                files));
        }
        sets.Sort((a, b) => a.OnlineId != b.OnlineId
            ? a.OnlineId.CompareTo(b.OnlineId)
            : string.CompareOrdinal(a.Id, b.Id));
        return sets;
    }

    static BeatmapRecord ReadBeatmap(IRealmObject beatmap)
    {
        var api = beatmap.DynamicApi;
        var metadata = api.Get<IRealmObject>("Metadata");
        var author = metadata?.DynamicApi.Get<IEmbeddedObject>("Author");
        var difficulty = api.Get<IEmbeddedObject>("Difficulty");
        var ruleset = api.Get<IRealmObject>("Ruleset");
        string Meta(string field) => metadata?.DynamicApi.Get<string>(field);

        return new BeatmapRecord(
            api.Get<Guid>("ID").ToString(),
            api.Get<int>("OnlineID"),
            api.Get<string>("Hash"),
            api.Get<string>("MD5Hash"),
            api.Get<string>("DifficultyName"),
            ruleset?.DynamicApi.Get<int>("OnlineID") ?? 0,
            api.Get<double>("Length"),
            api.Get<double>("BPM"),
            api.Get<double>("StarRating"),
            api.Get<int>("Status"),
            api.Get<bool>("Hidden"),
            Meta("Title"),
            Meta("TitleUnicode"),
            Meta("Artist"),
            Meta("ArtistUnicode"),
            author?.DynamicApi.Get<string>("Username"),
            Meta("Source"),
            Meta("Tags"),
            difficulty?.DynamicApi.Get<float>("DrainRate") ?? 0,
            difficulty?.DynamicApi.Get<float>("CircleSize") ?? 0,
            difficulty?.DynamicApi.Get<float>("OverallDifficulty") ?? 0,
            difficulty?.DynamicApi.Get<float>("ApproachRate") ?? 0,
            difficulty?.DynamicApi.Get<double>("SliderMultiplier") ?? 0,
            difficulty?.DynamicApi.Get<double>("SliderTickRate") ?? 0);
    }

    static void Write(List<SetRecord> sets, Stream output)
    {
        using var w = new Utf8JsonWriter(output, new JsonWriterOptions { Indented = false });
        w.WriteStartArray();
        foreach (var set in sets)
        {
            w.WriteStartObject();
            w.WriteString("id", set.Id);
            w.WriteNumber("online_id", set.OnlineId);
            w.WriteBoolean("protected", set.Protected);
            w.WriteBoolean("delete_pending", set.DeletePending);
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
                w.WriteNumber("length_ms", b.LengthMs);
                w.WriteNumber("bpm", b.Bpm);
                w.WriteNumber("star_rating", b.StarRating);
                w.WriteNumber("status", b.Status);
                w.WriteBoolean("hidden", b.Hidden);
                w.WriteString("title", b.Title);
                w.WriteString("title_unicode", b.TitleUnicode);
                w.WriteString("artist", b.Artist);
                w.WriteString("artist_unicode", b.ArtistUnicode);
                w.WriteString("author", b.Author);
                w.WriteString("source", b.Source);
                w.WriteString("tags", b.Tags);
                w.WriteNumber("drain_rate", b.DrainRate);
                w.WriteNumber("circle_size", b.CircleSize);
                w.WriteNumber("overall_difficulty", b.OverallDifficulty);
                w.WriteNumber("approach_rate", b.ApproachRate);
                w.WriteNumber("slider_multiplier", b.SliderMultiplier);
                w.WriteNumber("slider_tick_rate", b.SliderTickRate);
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
        w.WriteEndArray();
    }
}

record SetRecord(
    string Id, int OnlineId, bool Protected, bool DeletePending,
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
