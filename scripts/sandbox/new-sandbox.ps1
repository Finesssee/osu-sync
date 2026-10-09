<#
.SYNOPSIS
Builds a writable osu-sync test sandbox from copies of the live installs.

.DESCRIPTION
Copies the lazer blobs of the first -Sets sets listed in -SetsJson into
<Root>\lazer\files, copies the realm to <Root>\lazer\client.realm, writes
<Root>\lazer\sets.json with those sets, and creates <Root>\stable\Songs plus a
copy of osu!.db. The live folders are only read. Files that already exist with
the same size and modified time are skipped, so a second run copies nothing.
Root must be a local drive path outside the live installs and %APPDATA%\osu, and
must not contain either source folder.

.EXAMPLE
pwsh -NoProfile -File scripts/sandbox/new-sandbox.ps1 -Root D:\osu-sync-sandbox\lane-1 -Sets 40
#>
param(
    [Parameter(Mandatory)][string]$Root,
    [Parameter(Mandatory)][ValidateRange(0, [int]::MaxValue)][int]$Sets,
    [string]$SetsJson = 'W:\swarm\fixtures\sets.json',
    [string]$LazerSource = 'D:\osu!lazer',
    [string]$StableSource = 'D:\osu!',
    [string]$RealmSource = 'W:\swarm\fixtures\client-copy.realm'
)

$ErrorActionPreference = 'Stop'

function Get-FullPath([string]$Path) {
    [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
}

function Test-Under([string]$Path, [string]$Parent) {
    $p = (Get-FullPath $Path) -split '[\\/]'
    $r = (Get-FullPath $Parent) -split '[\\/]'
    if ($p.Count -lt $r.Count) { return $false }
    for ($i = 0; $i -lt $r.Count; $i++) {
        if ($p[$i] -ine $r[$i]) { return $false }
    }
    $true
}

function Stop-Refused([string]$Message) {
    [Console]::Error.WriteLine($Message)
    exit 2
}

if ($Root -match '^[\\/]') {
    Stop-Refused "Refusing to build a sandbox at $Root because it does not start with a drive letter"
}
$Root = Get-FullPath $Root

$liveFolders = @('D:\osu!', 'D:\osu!lazer', $LazerSource, $StableSource)
if ($env:APPDATA) { $liveFolders += Join-Path $env:APPDATA 'osu' }
foreach ($live in $liveFolders) {
    if (Test-Under $Root $live) {
        Stop-Refused "Refusing to build a sandbox at $Root because it is inside the live folder $live"
    }
}
foreach ($source in $LazerSource, $StableSource) {
    if (Test-Under $source $Root) {
        Stop-Refused "Refusing to build a sandbox at $Root because it contains the source folder $source"
    }
}

$stats = [ordered]@{ copied = 0; skipped = 0 }

function Copy-IfChanged([string]$Source, [string]$Dest) {
    $src = Get-Item -LiteralPath $Source
    $dst = Get-Item -LiteralPath $Dest -ErrorAction SilentlyContinue
    if ($dst -and $dst.Length -eq $src.Length -and $dst.LastWriteTimeUtc -eq $src.LastWriteTimeUtc) {
        $stats.skipped++
        return
    }
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Dest))
    [IO.File]::Copy($Source, $Dest, $true)
    $stats.copied++
}

$started = Get-Date
$allSets = Get-Content -LiteralPath $SetsJson -Raw | ConvertFrom-Json
$chosen = @($allSets | Select-Object -First $Sets)
if ($chosen.Count -lt $Sets) {
    [Console]::Error.WriteLine("$SetsJson lists only $($chosen.Count) sets, fewer than -Sets $Sets")
    exit 1
}

$lazerRoot = Join-Path $Root 'lazer'
$stableRoot = Join-Path $Root 'stable'
[void][IO.Directory]::CreateDirectory((Join-Path $lazerRoot 'files'))
[void][IO.Directory]::CreateDirectory((Join-Path $stableRoot 'Songs'))

$hashes = $chosen | ForEach-Object { $_.files } | ForEach-Object { $_.hash } | Sort-Object -Unique
foreach ($hash in $hashes) {
    $rel = Join-Path $hash.Substring(0, 1) (Join-Path $hash.Substring(0, 2) $hash)
    Copy-IfChanged (Join-Path $LazerSource "files\$rel") (Join-Path $lazerRoot "files\$rel")
}

Copy-IfChanged $RealmSource (Join-Path $lazerRoot 'client.realm')
Copy-IfChanged (Join-Path $StableSource 'osu!.db') (Join-Path $stableRoot 'osu!.db')

$setsOut = Join-Path $lazerRoot 'sets.json'
$json = ConvertTo-Json -InputObject $chosen -Depth 5 -Compress
$existing = if (Test-Path -LiteralPath $setsOut) { [IO.File]::ReadAllText($setsOut) }
if ($existing -ceq $json) {
    $stats.skipped++
} else {
    [IO.File]::WriteAllText($setsOut, $json, [Text.UTF8Encoding]::new($false))
    $stats.copied++
}

$seconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 2)
"sandbox $Root sets=$Sets blobs=$($hashes.Count) copied=$($stats.copied) skipped=$($stats.skipped) seconds=$seconds"
