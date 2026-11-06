# Vendor-Holen.ps1 - holt eine Fassung eines Toolkit-Pakets zur Durchsicht.
#
# Etappe 3a-3, Entscheidung ui-toolkit-als-vendor.
# Das Skript schreibt NIE in web\vendor. Es legt einen neuen, leeren Ordner an
# (Standard: %TEMP%\vendor-holen\<paket>-<version>-<zeit>) und tut dort:
#
#   1. npm pack <paket>@<version> (nur der Tarball aus der Registry, keine
#      Paket-Skripte)
#   2. Prüfsumme des Tarballs (sha512) gegen dist.integrity der Registry
#   3. Auspacken nach <ordner>\<paket>, Markdown-Dateien entfernen, eine
#      LICENSE.md inhaltsgleich zu LICENSE.txt umbenennen
#   4. Lizenz prüfen (MIT, Apache-2.0, BSD, 0BSD, ISC) und HERKUNFT.toml
#      schreiben (Version, Datum, Prüfsummen, entfernte Dateien)
#   5. Abhängigkeiten der neuen Fassung neben die heutige Kopie stellen
#
# Danach von Hand, in einer Katalog-Runde (Anleitung, „Toolkit als Vendor-Kopie“):
# Unterschiede ansehen, den Ordner web\vendor\<paket> durch <ordner>\<paket>
# ersetzen, in web\ npm install --offline, npm run verify, Sichtprüfung im
# /katalog, scripts\Vendor-Pruefen.ps1, ein Commit je Paket.
#
# Aufruf (PowerShell 7 oder Windows PowerShell 5.1, kein Fenster):
#   pwsh -NoProfile -File scripts\Vendor-Holen.ps1 -Paket bits-ui -Version 2.20.3
#   pwsh -NoProfile -File scripts\Vendor-Holen.ps1 -Paket @floating-ui/dom -Version latest
#
# Exit-Codes:
#   0  Geholt und geprüft; der Ordner steht in der letzten Zeile.
#   1  Prüfsumme oder Lizenz passt nicht (Ordner bleibt zur Ansicht).
#   2  Aufruf falsch, Ziel nicht leer oder Ziel liegt in web\vendor.
#   3  npm fehlt oder die Registry ist nicht erreichbar.
param(
    [Parameter(Mandatory = $true)][string]$Paket,
    [string]$Version = 'latest',
    [string]$Ziel = '',
    [string]$Repo = ''
)
$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 kennt $PSScriptRoot im param-Block nicht immer.
if (-not $Repo) { $Repo = Split-Path -Parent $PSScriptRoot }
# Satz auf stderr und Ende mit Code (Write-Error endete mit Stop immer mit 1).
function Abbruch([string]$satz, [int]$code) { [Console]::Error.WriteLine($satz); exit $code }

if ($Paket -notmatch '^(@[a-z0-9][a-z0-9._-]*/)?[a-z0-9][a-z0-9._-]*$') {
    Abbruch "Unbekannte Form des Paketnamens: $Paket" 2
}
$ERLAUBT = @('MIT', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', '0BSD', 'ISC')

function Npm-Ausgabe([string[]]$argumente, [string]$ordner, [int]$sekunden = 120) {
    $node = (Get-Command node -ErrorAction SilentlyContinue).Source
    if (-not $node) { return $null }
    $cli = Join-Path (Split-Path -Parent $node) 'node_modules\npm\bin\npm-cli.js'
    if (-not (Test-Path -LiteralPath $cli)) { return $null }
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $node
    $psi.Arguments = (@("`"$cli`"") + ($argumente | ForEach-Object { "`"$_`"" })) -join ' '
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.WorkingDirectory = $ordner
    $p = [Diagnostics.Process]::Start($psi)
    $aus = $p.StandardOutput.ReadToEndAsync()
    [void]$p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit($sekunden * 1000)) {
        try { $p.Kill() } catch { }
        return $null
    }
    if ($p.ExitCode -ne 0) { return $null }
    return $aus.Result
}

function Inhalt-Sha256([string]$ordner) {
    # Gleiche Rechnung wie in Vendor-Pruefen.ps1.
    $basis = (Resolve-Path -LiteralPath $ordner).Path.TrimEnd('\')
    [string[]]$rel = @(Get-ChildItem -LiteralPath $basis -Recurse -File -Force |
        ForEach-Object { $_.FullName.Substring($basis.Length + 1).Replace('\', '/') } |
        Where-Object { $_ -ne 'HERKUNFT.toml' })
    [Array]::Sort($rel, [StringComparer]::Ordinal)
    $sb = New-Object System.Text.StringBuilder
    foreach ($r in $rel) {
        $h = (Get-FileHash -LiteralPath (Join-Path $basis $r) -Algorithm SHA256).Hash.ToLowerInvariant()
        [void]$sb.Append("${r}:$h`n")
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $bytes = $sha.ComputeHash((New-Object Text.UTF8Encoding $false).GetBytes($sb.ToString())) }
    finally { $sha.Dispose() }
    return @{ summe = (($bytes | ForEach-Object { $_.ToString('x2') }) -join ''); dateien = $rel }
}

function Toml-Text([string]$s) { return '"' + $s.Replace('\', '\\').Replace('"', '\"') + '"' }

# Ziel: neu und leer, nie in web\vendor.
$vendor = [IO.Path]::GetFullPath((Join-Path $Repo 'web\vendor'))
$sicher = ($Paket -replace '^@', '') -replace '/', '-'
if (-not $Ziel) {
    $Ziel = Join-Path $env:TEMP ("vendor-holen\{0}-{1}-{2}" -f $sicher, $Version, (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$Ziel = [IO.Path]::GetFullPath($Ziel)
if ($Ziel.StartsWith($vendor, [StringComparison]::OrdinalIgnoreCase)) {
    Abbruch "Das Ziel liegt in web\vendor. Dieses Skript ersetzt nie selbst; bitte einen anderen Ordner." 2
}
if ((Test-Path -LiteralPath $Ziel) -and @(Get-ChildItem -LiteralPath $Ziel -Force).Count -gt 0) {
    Abbruch "Das Ziel ist nicht leer: $Ziel" 2
}
New-Item -ItemType Directory -Force -Path $Ziel | Out-Null
$tarOrdner = Join-Path $Ziel 'tarball'
New-Item -ItemType Directory -Force -Path $tarOrdner | Out-Null

# 1. Metadaten und Tarball
$meta = Npm-Ausgabe @('view', "$Paket@$Version", 'version', 'dist.integrity', 'dist.tarball', 'license', 'dependencies', 'peerDependencies', '--json') $Ziel
if ($null -eq $meta) {
    Abbruch "npm fehlt oder die Registry antwortet nicht ($Paket@$Version)." 3
}
$m = $meta | ConvertFrom-Json
if ($m -is [array]) { $m = $m[-1] }
$ver = [string]$m.version
$integritaet = [string]$m.'dist.integrity'
$packJson = Npm-Ausgabe @('pack', "$Paket@$ver", '--ignore-scripts', '--json', '--pack-destination', $tarOrdner) $Ziel
if ($null -eq $packJson) {
    Abbruch "npm pack $Paket@$ver ist gescheitert." 3
}
$tgz = Get-ChildItem -LiteralPath $tarOrdner -Filter '*.tgz' | Select-Object -First 1

# 2. Prüfsumme selbst rechnen
$sha512 = [Security.Cryptography.SHA512]::Create()
try {
    $strom = [IO.File]::OpenRead($tgz.FullName)
    try { $sri = 'sha512-' + [Convert]::ToBase64String($sha512.ComputeHash($strom)) } finally { $strom.Dispose() }
}
finally { $sha512.Dispose() }
$fehler = @()
if ($sri -ne $integritaet) { $fehler += "Prüfsumme passt nicht: $sri statt $integritaet" }

# 3. Auspacken (tar von Windows, kein Fenster), Markdown entfernen
$paketOrdner = Join-Path $Ziel ($Paket -replace '/', '\')
New-Item -ItemType Directory -Force -Path $paketOrdner | Out-Null
$tar = Join-Path $env:SystemRoot 'System32\tar.exe'
$psi = New-Object Diagnostics.ProcessStartInfo
$psi.FileName = $tar
$psi.Arguments = "-xzf `"$($tgz.FullName)`" -C `"$paketOrdner`" --strip-components=1"
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$tp = [Diagnostics.Process]::Start($psi)
$tp.WaitForExit()
if ($tp.ExitCode -ne 0) {
    Abbruch "Auspacken ist gescheitert (tar $($tp.ExitCode))." 1
}
$basis = (Resolve-Path -LiteralPath $paketOrdner).Path.TrimEnd('\')
$vorher = @(Get-ChildItem -LiteralPath $basis -Recurse -File -Force)
$entfernt = @()
$umbenannt = @()
foreach ($f in $vorher) {
    if ($f.Name -notmatch '\.md$') { continue }
    $rel = $f.FullName.Substring($basis.Length + 1).Replace('\', '/')
    if ($rel -match '^licen[cs]e\.md$') {
        $neu = $rel -replace '\.md$', '.txt'
        Rename-Item -LiteralPath $f.FullName -NewName ($f.Name -replace '\.md$', '.txt')
        $umbenannt += "$rel -> $neu"
    }
    else {
        Remove-Item -LiteralPath $f.FullName
        $entfernt += $rel
    }
}

# 4. Lizenz und HERKUNFT.toml
$pj = Get-Content -LiteralPath (Join-Path $basis 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$lizenz = [string]$pj.license
$inhalt = Inhalt-Sha256 $basis
$lizDatei = @($inhalt.dateien | Where-Object { $_ -match '^(licen[cs]e|copying)(\.txt)?$' }) | Select-Object -First 1
if (-not $lizDatei) { $fehler += 'Keine Lizenzdatei im Paket.' }
if ($ERLAUBT -notcontains $lizenz) {
    $fehler += "Lizenz '$lizenz' steht nicht auf der Liste. Lizenzdatei lesen; ist es erlaubt, lizenz und lizenz_hinweis in HERKUNFT.toml von Hand setzen."
}
$toml = @(
    '# Herkunft dieser Vendor-Kopie (Entscheidung ui-toolkit-als-vendor).',
    '# Inhalt unverändert aus der npm-Registry; nur Markdown entfernt (siehe entfernt).',
    "paket = $(Toml-Text $Paket)",
    "version = $(Toml-Text $ver)",
    "quelle = $(Toml-Text ([string]$m.'dist.tarball'))",
    "datum = $(Toml-Text (Get-Date -Format 'yyyy-MM-dd'))",
    '# Prüfsumme des Tarballs aus der Registry (npm integrity, sha512, base64); vor dem Auspacken nachgerechnet.',
    "pruefsumme = $(Toml-Text $sri)",
    "lizenz = $(Toml-Text $lizenz)",
    "lizenzdatei = $(Toml-Text ([string]$lizDatei))",
    "dateien_im_tarball = $($vorher.Count)",
    "entfernt = [$((@($entfernt | Sort-Object) | ForEach-Object { Toml-Text $_ }) -join ', ')]",
    "umbenannt = [$(($umbenannt | ForEach-Object { Toml-Text $_ }) -join ', ')]",
    '# Prüfsumme der Kopie (ohne HERKUNFT.toml), Rechnung siehe scripts/Vendor-Pruefen.ps1.',
    "dateien = $($inhalt.dateien.Count)",
    "inhalt_sha256 = $(Toml-Text $inhalt.summe)"
) -join "`n"
[IO.File]::WriteAllText((Join-Path $basis 'HERKUNFT.toml'), $toml + "`n", (New-Object Text.UTF8Encoding $false))

# 5. Abhängigkeiten neben die heutige Kopie stellen
Write-Host "$Paket $ver (Lizenz $lizenz, $($inhalt.dateien.Count) Dateien, $($entfernt.Count) Markdown entfernt)"
$heute = Join-Path $vendor ($Paket -replace '/', '\')
if (Test-Path -LiteralPath (Join-Path $heute 'HERKUNFT.toml')) {
    $alt = (Select-String -LiteralPath (Join-Path $heute 'HERKUNFT.toml') -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
    Write-Host "Heute in web\vendor: $alt"
}
foreach ($teil in @('dependencies', 'peerDependencies')) {
    $abh = $pj.$teil
    if ($null -eq $abh) { continue }
    foreach ($e in $abh.PSObject.Properties) {
        $hk = Join-Path $vendor (($e.Name -replace '/', '\') + '\HERKUNFT.toml')
        $da = if (Test-Path -LiteralPath $hk) {
            'heute ' + (Select-String -LiteralPath $hk -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
        } else { 'nicht in web\vendor' }
        Write-Host ("  {0,-17} {1} {2} ({3})" -f $teil, $e.Name, $e.Value, $da)
    }
}
foreach ($f in $fehler) { Write-Host "FEHLER: $f" }
Write-Host $paketOrdner
if ($fehler.Count -gt 0) { exit 1 }
exit 0
