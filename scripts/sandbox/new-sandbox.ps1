<#
.SYNOPSIS
Builds a writable osu-sync test sandbox from copies of the live installs.

.DESCRIPTION
Exports -RealmSource with realm-export, takes the first -Sets sets that are not
pending deletion, and copies their lazer blobs into <Root>\lazer\files. It copies
the realm to <Root>\lazer\client.realm and trims it to those sets with
`realm-export trim`, writes <Root>\lazer\sets.json with the exported records of
those sets, and creates <Root>\stable\Songs plus a copy of osu!.db. The live
folders are only read. Files that already exist with the same size and modified
time are skipped, and a trimmed realm that still matches <Root>\lazer\trim.stamp
is kept, so a second run copies nothing.
Root must be under D:\osu-sync-sandbox, because realm-export trims nothing
outside it. Root must not be inside the live installs or %APPDATA%\osu, must not
contain either source folder, may not be on a network or subst drive, and no
existing folder on its path may be a junction or symbolic link.
-RealmExport defaults to OSU_SYNC_REALM_EXPORT, then to
target\release\realm-export.exe in this repository.

.EXAMPLE
pwsh -NoProfile -File scripts/sandbox/new-sandbox.ps1 -Root D:\osu-sync-sandbox\lane-1 -Sets 40
#>
param(
    [Parameter(Mandatory)][string]$Root,
    [Parameter(Mandatory)][ValidateRange(0, [int]::MaxValue)][int]$Sets,
    [string]$LazerSource = 'D:\osu!lazer',
    [string]$StableSource = 'D:\osu!',
    [string]$RealmSource = 'W:\swarm\fixtures\client-copy.realm',
    [string]$RealmExport = $(if ($env:OSU_SYNC_REALM_EXPORT) { $env:OSU_SYNC_REALM_EXPORT } else { Join-Path $PSScriptRoot '..\..\target\release\realm-export.exe' })
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
if (-not (Test-Under $Root 'D:\osu-sync-sandbox')) {
    Stop-Refused "Refusing to build a sandbox at $Root because it is outside D:\osu-sync-sandbox"
}

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

# The checks above compare text, so a network drive, a subst drive or a junction
# could still lead into a live folder. Refuse all three rather than resolve them.
$drive = [IO.Path]::GetPathRoot("$Root\")
if ([IO.DriveInfo]::new($drive).DriveType -eq [IO.DriveType]::Network) {
    Stop-Refused "Refusing to build a sandbox at $Root because $drive is a network drive"
}
$letter = $drive.Substring(0, 1).ToUpperInvariant()
foreach ($line in @(subst.exe)) {
    if ($line -match '^([A-Za-z]):\\: =>' -and $Matches[1].ToUpperInvariant() -eq $letter) {
        Stop-Refused "Refusing to build a sandbox at $Root because ${letter}: is a subst drive ($line)"
    }
}
for ($p = $Root; $p; $p = [IO.Path]::GetDirectoryName($p)) {
    $item = Get-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue
    if ($item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        Stop-Refused "Refusing to build a sandbox at $Root because $p is a junction or symbolic link"
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

function Invoke-RealmExport([string[]]$Arguments) {
    $info = [Diagnostics.ProcessStartInfo]::new($RealmExport)
    foreach ($arg in $Arguments) { $info.ArgumentList.Add($arg) }
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
    $process = [Diagnostics.Process]::Start($info)
    $errTask = $process.StandardError.ReadToEndAsync()
    $out = $process.StandardOutput.ReadToEnd()
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) {
        [Console]::Error.WriteLine("realm-export $($Arguments[0]) failed with exit code $($process.ExitCode): $($errTask.Result.Trim())")
        exit 1
    }
    $out
}

function Get-Stamp([string]$Path) {
    $item = Get-Item -LiteralPath $Path -ErrorAction SilentlyContinue
    if ($item) { "$($item.Length) $($item.LastWriteTimeUtc.Ticks)" }
}

if (-not (Test-Path -LiteralPath $RealmExport -PathType Leaf)) {
    [Console]::Error.WriteLine("realm-export not found at $RealmExport. Build it with dotnet publish tools/realm-export -c Release -o target/release, or pass -RealmExport")
    exit 1
}

$started = Get-Date
$export = [Text.Json.JsonDocument]::Parse((Invoke-RealmExport @('export', $RealmSource)))
$chosen = @($export.RootElement.EnumerateArray() |
    Where-Object { -not $_.GetProperty('delete_pending').GetBoolean() } |
    Select-Object -First $Sets)
if ($chosen.Count -lt $Sets) {
    [Console]::Error.WriteLine("$RealmSource holds only $($chosen.Count) sets, fewer than -Sets $Sets")
    exit 1
}

$lazerRoot = Join-Path $Root 'lazer'
$stableRoot = Join-Path $Root 'stable'
[void][IO.Directory]::CreateDirectory((Join-Path $lazerRoot 'files'))
[void][IO.Directory]::CreateDirectory((Join-Path $stableRoot 'Songs'))

$hashes = $chosen | ForEach-Object { $_.GetProperty('files').EnumerateArray() } |
    ForEach-Object { $_.GetProperty('hash').GetString() } | Sort-Object -Unique
foreach ($hash in $hashes) {
    $rel = Join-Path $hash.Substring(0, 1) (Join-Path $hash.Substring(0, 2) $hash)
    Copy-IfChanged (Join-Path $LazerSource "files\$rel") (Join-Path $lazerRoot "files\$rel")
}

$ids = @($chosen | ForEach-Object { $_.GetProperty('id').GetString() })
$realmOut = Join-Path $lazerRoot 'client.realm'
$stampOut = Join-Path $lazerRoot 'trim.stamp'
$wanted = "$(Get-Stamp $RealmSource) $($ids -join ',')"
$stamp = if (Test-Path -LiteralPath $stampOut) { [IO.File]::ReadAllLines($stampOut) }
if ($stamp -and $stamp[0] -ceq $wanted -and $stamp[1] -ceq (Get-Stamp $realmOut)) {
    $stats.skipped++
} else {
    $keepOut = Join-Path $lazerRoot 'trim-keep.txt'
    [IO.File]::WriteAllLines($keepOut, [string[]]$ids)
    [IO.File]::Copy($RealmSource, $realmOut, $true)
    [void](Invoke-RealmExport @('trim', $realmOut, '--keep', $keepOut))
    [IO.File]::WriteAllLines($stampOut, [string[]]@($wanted, (Get-Stamp $realmOut)))
    $stats.copied++
}

Copy-IfChanged (Join-Path $StableSource 'osu!.db') (Join-Path $stableRoot 'osu!.db')

$setsOut = Join-Path $lazerRoot 'sets.json'
$json = '[' + (($chosen | ForEach-Object { $_.GetRawText() }) -join ',') + ']'
$existing = if (Test-Path -LiteralPath $setsOut) { [IO.File]::ReadAllText($setsOut) }
if ($existing -ceq $json) {
    $stats.skipped++
} else {
    [IO.File]::WriteAllText($setsOut, $json, [Text.UTF8Encoding]::new($false))
    $stats.copied++
}

$seconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 2)
"sandbox $Root sets=$Sets blobs=$($hashes.Count) copied=$($stats.copied) skipped=$($stats.skipped) seconds=$seconds"
