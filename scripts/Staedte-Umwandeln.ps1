#Requires -Version 7.7
<#
.SYNOPSIS
    Wandelt die Städteliste von GeoNames (cities15000, 19 Spalten) in die gekürzte
    Liste des Kalenders (7 Spalten) und schreibt die Kopfdatei mit Prüfsummen.
.DESCRIPTION
    Eingang: die heruntergeladene Datei cities15000.zip (oder die entpackte
    cities15000.txt) als Daten; nichts darin wird ausgeführt. Ausgang im privaten
    Inhalt: projects/kalender/staedte/staedte.tsv (Name, Nebennamen, Land,
    Region, Breite, Länge, Zeitzone; Tab-getrennt) und staedte-kopf.toml mit
    quelle, abgerufen, lizenz („CC BY 4.0“), sha256 (der heruntergeladenen Datei)
    und sha256_liste (der gekürzten Liste). Den Kopf prüft der Server beim Laden
    (src/kalender/ortsliste.rs).

    GeoNames-Spalten (readme.txt): 0 geonameid, 1 name, 2 asciiname,
    3 alternatenames, 4 latitude, 5 longitude, … 8 country code, … 10 admin1 code,
    … 17 timezone, 18 modification date.
.PARAMETER Quelle
    Pfad zur heruntergeladenen cities15000.zip oder cities15000.txt.
.PARAMETER Inhalt
    Wurzel des privaten Inhalts. Standard: DASHBOARD_INHALT, sonst der
    Nachbarordner dashboard-inhalt des Code-Repos. Das Projekt kalender muss
    darin schon bestehen (projects\kalender\projekt.toml).
.PARAMETER Abgerufen
    Tag des Herunterladens (JJJJ-MM-TT), Standard heute.
.PARAMETER Adresse
    Woher die Eingabe stammt, für `quelle` im Kopf. Standard: bei einer ZIP-Datei
    die Download-Adresse von GeoNames, bei einer Textdatei ihr Dateiname (GeoNames
    veröffentlicht nur die ZIP-Datei).
.PARAMETER Sha256
    Die Prüfsumme, die der Download nennt; weicht die der Eingabe ab, wird nichts
    geschrieben.
#>
param(
    [Parameter(Mandatory)][string]$Quelle,
    [string]$Inhalt,
    [string]$Abgerufen = (Get-Date -Format 'yyyy-MM-dd'),
    [string]$Adresse,
    [string]$Sha256
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Laufzeit.ps1')
$uhr = UhrStart

$repo = Split-Path -Parent $PSScriptRoot
if (-not $Inhalt) { $Inhalt = if ($env:DASHBOARD_INHALT) { $env:DASHBOARD_INHALT } else { Join-Path (Split-Path -Parent $repo) 'dashboard-inhalt' } }
if (-not (Test-Path -LiteralPath (Join-Path $Inhalt 'projects\kalender\projekt.toml') -PathType Leaf)) {
    throw "Im Inhalt $Inhalt gibt es kein Projekt kalender (projects\kalender\projekt.toml fehlt); nichts geschrieben."
}
$istZip = $Quelle -like '*.zip'
if (-not $Adresse) { $Adresse = if ($istZip) { 'https://download.geonames.org/export/dump/cities15000.zip' } else { "lokale Datei $(Split-Path -Leaf $Quelle)" } }

$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Quelle).Hash.ToLowerInvariant()
if ($Sha256 -and $hash -ne $Sha256.Trim().ToLowerInvariant()) {
    throw "Die Prüfsumme der Eingabe ($hash) weicht von -Sha256 ab; nichts geschrieben."
}
# Der Text der Liste: aus der ZIP-Datei der einzige .txt-Eintrag, sonst die Datei selbst.
if ($istZip) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Quelle)
    try {
        $eintrag = @($zip.Entries | Where-Object { $_.FullName -like '*.txt' })
        if ($eintrag.Count -ne 1) { throw 'Die ZIP-Datei enthält nicht genau eine Textdatei.' }
        $leser = [IO.StreamReader]::new($eintrag[0].Open(), [Text.UTF8Encoding]::new($false))
        try { $text = $leser.ReadToEnd() } finally { $leser.Dispose() }
    } finally { $zip.Dispose() }
} else {
    $text = [IO.File]::ReadAllText($Quelle, [Text.UTF8Encoding]::new($false))
}

$zeilen = $text.Split("`n", [StringSplitOptions]::RemoveEmptyEntries)
$aus = [Text.StringBuilder]::new($text.Length / 2)
$uebersprungen = 0
foreach ($z in $zeilen) {
    $f = $z.TrimEnd("`r").Split("`t")
    # Eine Zeile ohne 19 Spalten oder ohne Zone gehört nicht in die Liste.
    if ($f.Count -ne 19 -or -not $f[17]) { $uebersprungen++; continue }
    [void]$aus.Append($f[1]).Append("`t").Append($f[3]).Append("`t").Append($f[8]).Append("`t").Append($f[10]).Append("`t").Append($f[4]).Append("`t").Append($f[5]).Append("`t").Append($f[17]).Append("`n")
}

$ziel = Join-Path $Inhalt 'projects\kalender\staedte'
[void][IO.Directory]::CreateDirectory($ziel)
$liste = Join-Path $ziel 'staedte.tsv'
[IO.File]::WriteAllText($liste, $aus.ToString(), [Text.UTF8Encoding]::new($false))
$hashListe = (Get-FileHash -Algorithm SHA256 -LiteralPath $liste).Hash.ToLowerInvariant()
$kopf = @"
quelle = "$($Adresse.Replace('\', '\\').Replace('"', '\"'))"
abgerufen = $Abgerufen
lizenz = "CC BY 4.0"
sha256 = "$hash"
sha256_liste = "$hashListe"
"@
[IO.File]::WriteAllText((Join-Path $ziel 'staedte-kopf.toml'), $kopf.Replace("`r`n", "`n") + "`n", [Text.UTF8Encoding]::new($false))
"Städte: $($zeilen.Count - $uebersprungen), übersprungen: $uebersprungen."
"Liste und Kopf liegen in $ziel."
UhrZeile $uhr
