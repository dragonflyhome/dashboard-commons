<#
.SYNOPSIS
    Zieht den privaten Inhalt aus dem Code-Repo in ein eigenes, nur lokales Repo
    dashboard-inhalt (Umzug des Inhalts).
.DESCRIPTION
    Entscheidungen: github-trennung (mit [docs_oeffentlich]),
    inhaltsquellen ([zusammenbau], [code_nur_kern]),
    Nachbesserung abnahme-umzug-inhalt. Das Skript löscht nie
    Daten. Es läuft in zwei Durchgängen:

    Erster Durchgang (ohne -SicherungBestaetigt):
      1. Prüft: Code-Repo sauber an den Orten des Inhalts (nichts
         Uncommittetes); der Inhalts-Ordner liegt neben dem Code-Repo, ist neu
         (oder leer) und sicher (Inhalt-Unsicher: absolut, im Profil, ohne ..,
         keine Verknüpfung auf dem Weg).
      2. Ausschlussliste des Inventars: data\inventar-ausschluss.toml, sonst
         aus dem letzten Stand von src\pc\inventory.rs, der die Liste noch im
         Code trug (git). Ohne Liste bricht es ab (geschlossen scheitern).
      3. Sammelt projects\ (ohne _vorlage), data\, docs\entscheidungen\ und
         docs\architektur-stand.md (eingecheckt und nicht ignoriert). Trägt
         irgendein Pfad dort einen Tabu-Namen (eingecheckt, neu oder von git
         ignoriert), bricht es ab: Es überspringt nichts, öffnet und listet
         solche Ordner nie (nur der Name eines Pfadteils wird verglichen).
         Was git ignoriert, wird genannt (es zieht nicht mit um).
         Dann kopiert es und vergleicht jede Kopie per SHA-256.
      4. Legt im Inhalt an: .gitignore, .gitattributes, UMZUG.txt (Herkunft,
         Prüfsummen, ignorierte Pfade), pruefer\namen.toml (allgemein, tabu,
         schluessel, rechner, benutzer vorbelegt; sensibel, nur_basis, wlan,
         mac trägt der Inhaber ein), pruefer\repos.toml (leer, nur
         wenn sie fehlt; Art je Remote-Adresse) und dashboard-lokal (aus
         vorlagen\dashboard-lokal).
      5. git init, erster Commit als Ranke, dann kisten.toml mit diesem Commit.
      6. Baut das Code-Repo (cargo build --release --locked), prüft
         dashboard-lokal mit dem Wächter aus dem Code-Repo (dashboard.exe
         pruefe-zusammenbau, liest nur), schreibt Cargo.lock mit cargo update
         --workspace --offline (löst nur auf, lädt und baut nichts), prüft
         noch einmal ganz und baut
         erst dann (cargo --config <code-repo>\.cargo\config.toml build
         --release --locked, Bauordner des Code-Repos; im Inhalt gibt es keine
         .cargo-Einstellung).
      7. Schreibt [inhalt] in die Einstellungen (-DienstToml, sonst nennt es
         den Befehl für den Inhaber als Administrator) und vermerkt in UMZUG.txt
         „Erster Durchgang fertig“ (eigener Commit).
    Bleibt der erste Durchgang stehen, sagt er genau, wie es weitergeht: Nach
    Schritt 5 macht ein zweiter Aufruf bei Schritt 6 weiter; davor heißt es,
    den halben Ordner umzubenennen (nicht löschen) und neu zu beginnen.

    Zweiter Durchgang (mit -SicherungBestaetigt, erst nach der Sicherung):
      8. Prüft: der erste Durchgang ist fertig; dienst.toml hat [inhalt] mit
         dem Inhalt; der Dienst liest wirklich daraus (/api/betrieb/voll,
         Feld quellen, über scripts\Api.ps1), und jeder Ordner, den er liest,
         meldet dort „lesbar“. Vergleicht noch einmal jede
         Kopie mit dem Original (SHA-256) und nennt, was git ignoriert.
      9. Legt im Code-Repo einen Zweig an (Standard umzug/inhalt-entfernen,
         in einem eigenen Worktree, der gemeinsame Arbeitsbaum bleibt
         unberührt): git rm der Orte des Inhalts (und der erzeugten Typen
         web\src\lib\vertrag\projekte), .gitignore dazu, ein Commit als Ranke.
         Aus dem Arbeitsbaum verschwinden die Dateien erst beim Merge dieses
         Zweigs (git merge --no-ff), den der Inhaber nach der Abnahme macht.

    Mit -Pruefen zeigt es nur, was es täte, und ändert nichts.
.PARAMETER Repo
    Das Code-Repo (Standard: der Ordner über scripts).
.PARAMETER Inhalt
    Der neue Inhalts-Ordner (Standard: dashboard-inhalt neben dem Code-Repo).
.PARAMETER Pruefen
    Trockenlauf: nichts schreiben, nur zeigen.
.PARAMETER SicherungBestaetigt
    Der Inhalt ist in der Sicherung (die Archiv-Rolle hat es bestätigt). Erst
    dann legt das Skript den Zweig an, der den Inhalt aus dem Code-Repo nimmt.
.PARAMETER Zweig
    Name des Zweigs für das Entfernen (Standard umzug/inhalt-entfernen).
.PARAMETER DienstToml
    Erster Durchgang: eine Einstellungsdatei, in die [inhalt] geschrieben wird
    (für Tests mit einer Wegwerf-Datei; die echte unter ProgramData schreibt nur
    Dienst-Einrichten.ps1 -NurLeserecht -Inhalt als Administrator). Zweiter
    Durchgang: die Datei, in der [inhalt] stehen muss (Standard
    C:\ProgramData\Dashboard\dienst.toml; nur gelesen).
.PARAMETER Server
    Zweiter Durchgang: Adresse des Dienstes für /api/betrieb/voll (Standard
    127.0.0.1 mit dem Port aus der Einstellungsdatei, sonst 8787).
.PARAMETER Bauordner
    Bauordner für cargo (Standard <Repo>\target; nur für Tests anders).
.PARAMETER OhneBau
    dashboard-lokal nicht bauen und nicht prüfen (nur für Tests ohne cargo).
#>
[CmdletBinding()]
param(
    [string]$Repo,
    [string]$Inhalt,
    [switch]$Pruefen,
    [switch]$SicherungBestaetigt,
    [string]$Zweig = 'umzug/inhalt-entfernen',
    [string]$DienstToml,
    [string]$Server,
    [string]$Bauordner,
    [switch]$OhneBau
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Inhalt-Orte.ps1')
$utf8 = New-Object System.Text.UTF8Encoding($false)
try { [Console]::OutputEncoding = $utf8 } catch { }

function Zeile([string]$t) { Write-Host $t }
# Abbruch mit Satz; $weiter sagt, wie es weitergeht (wenn es mehr als „beheben, noch einmal“ ist).
function Abbruch([string]$satz, [string[]]$weiter = @()) {
    Write-Host "Abbruch: $satz Es wurde nichts entfernt."
    if ($weiter.Count -gt 0) {
        Write-Host 'So geht es weiter:'
        foreach ($w in $weiter) { Write-Host "  $w" }
    }
    exit 1
}
function Pfad([string]$p) { $p -replace '/', '\' }
function Schreiben([string]$datei, [string]$text) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $datei) | Out-Null
    [IO.File]::WriteAllText($datei, $text, $utf8)
}
function Hash([string]$datei) { (Get-FileHash -LiteralPath $datei -Algorithm SHA256).Hash }

function GitAus([string]$wo, [string[]]$a) {
    $aus = & git -C $wo -c core.quotepath=off -c core.longpaths=true @a 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git $($a -join ' ') in $wo endete mit $LASTEXITCODE ($(($aus | Out-String).Trim()))" }
    return (($aus | ForEach-Object { "$_" }) -join "`n")
}
function GitRanke([string]$wo, [string[]]$a) {
    GitAus $wo (@('-c', 'user.name=Ranke', '-c', 'user.email=ranke2@proton.me') + $a)
}

if (-not $Repo) { $Repo = Split-Path -Parent $PSScriptRoot }
$Repo = (Resolve-Path -LiteralPath $Repo).Path.TrimEnd('\')
if (-not $Inhalt) { $Inhalt = Join-Path (Split-Path -Parent $Repo) 'dashboard-inhalt' }
$Inhalt = [IO.Path]::GetFullPath($Inhalt).TrimEnd('\')
if (-not $Bauordner) { $Bauordner = Join-Path $Repo 'target' }
$modus = if ($Pruefen) { 'nur prüfen, ändert nichts' } elseif ($SicherungBestaetigt) { 'zweiter Durchgang: Zweig zum Entfernen' } else { 'erster Durchgang: kopieren' }
Zeile "Inhalt-Umziehen.ps1 ($modus)"
Zeile "  Code-Repo: $Repo"
Zeile "  Inhalt:    $Inhalt"

# Die Befehle für „so geht es weiter“, mit denselben Angaben wie dieser Aufruf.
$extra = @()
foreach ($k in 'Repo', 'Inhalt', 'DienstToml', 'Server', 'Bauordner', 'Zweig') {
    if ($PSBoundParameters.ContainsKey($k)) { $extra += "-$k `"$((Get-Variable $k).Value)`"" }
}
if ($OhneBau) { $extra += '-OhneBau' }
$befehl = "pwsh -NoProfile -File `"$(Join-Path $PSScriptRoot 'Inhalt-Umziehen.ps1')`"$(if ($extra) { ' ' + ($extra -join ' ') })"
$befehlErster = $befehl
$befehlZweiter = "$befehl -SicherungBestaetigt"

# Stand des Inhalts-Ordners: neu, halb (Schritt 3 bis 5), nach Schritt 5, fertig.
function Stand {
    if (-not (Test-Path -LiteralPath $Inhalt)) { return 'neu' }
    $umzug = Join-Path $Inhalt 'UMZUG.txt'
    if (-not (Test-Path -LiteralPath $umzug)) {
        if (@(Get-ChildItem -LiteralPath $Inhalt -Force).Count -eq 0) { return 'neu' }
        return 'fremd'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $Inhalt '.git'))) { return 'halb' }
    $log = & git -C $Inhalt log --format=%s 2>$null
    if ($LASTEXITCODE -ne 0) { return 'halb' }
    $log = @($log)
    if ($log | Where-Object { $_ -eq 'Umzug: erster Durchgang fertig' }) { return 'fertig' }
    if ($log | Where-Object { $_ -like 'dashboard-lokal: geprüfte Kisten mit Commit *' }) { return 'nach5' }
    return 'halb'
}
$umbenennen = {
    $neuName = "$(Split-Path -Leaf $Inhalt)-halb-$('{0:yyyyMMdd-HHmm}' -f (Get-Date))"
    @(
        "1. Den halben Ordner umbenennen (nicht löschen; er enthält nur Kopien): Rename-Item -LiteralPath `"$Inhalt`" -NewName `"$neuName`"",
        "2. Den ersten Durchgang neu beginnen: $befehlErster",
        "3. Wenn er durchgelaufen ist, den umbenannten Ordner nach Rücksprache mit dem Inhaber wegräumen."
    )
}

try {
    # -----------------------------------------------------------------------
    # 1. Vorbedingungen
    # -----------------------------------------------------------------------
    if (-not (Test-Path -LiteralPath (Join-Path $Repo '.git'))) { Abbruch "$Repo ist kein Git-Repo." }
    if ((Split-Path -Parent $Inhalt) -ne (Split-Path -Parent $Repo)) {
        Abbruch 'Der Inhalts-Ordner muss neben dem Code-Repo liegen (gleicher Elternordner); sonst stimmen die Wege der Kisten zum SDK nicht.'
    }
    $u = Inhalt-Unsicher $Inhalt $env:USERPROFILE $Repo
    if ($u) { Abbruch $u }
    $offen = GitAus $Repo (@('status', '--porcelain', '--untracked-files=no', '--') + $InhaltZiehtUm)
    if ($offen.Trim()) { Abbruch 'An den Orten des Inhalts gibt es Uncommittetes (git status). Erst committen, dann umziehen.' }
    $quelle = (GitAus $Repo @('rev-parse', 'HEAD')).Trim()
    $kurz = $quelle.Substring(0, 9)
    $stand = Stand
    if ($stand -eq 'fremd') { Abbruch "$Inhalt gibt es schon und ist nicht leer (und nicht von diesem Skript, es fehlt UMZUG.txt). Es wird nichts überschrieben." }
    if ($SicherungBestaetigt -and $stand -ne 'fertig') {
        switch ($stand) {
            'neu' { Abbruch 'Der zweite Durchgang braucht den ersten (es gibt noch keinen Inhalts-Ordner).' @("Erst den ersten Durchgang: $befehlErster") }
            'nach5' { Abbruch 'Der erste Durchgang ist nach Schritt 5 stehen geblieben (Repo angelegt, Kisten eingetragen; Bau oder Einstellungen fehlen).' @("1. Den ersten Durchgang noch einmal; er macht bei Schritt 6 weiter: $befehlErster", "2. Danach Sicherung, als Administrator Dienst-Einrichten.ps1 -NurLeserecht -Inhalt und scripts\Dienst.ps1 -Neustart, dann: $befehlZweiter") }
            default { Abbruch 'Der erste Durchgang ist in Schritt 3 bis 5 stehen geblieben (Kopien ohne fertiges Repo).' (& $umbenennen) }
        }
    }

    # -----------------------------------------------------------------------
    # 2. Ausschlussliste des Inventars (nie im Code-Repo; geschlossen scheitern)
    # -----------------------------------------------------------------------
    $ausschlussQuelle = Join-Path $Repo 'data\inventar-ausschluss.toml'
    $ausschlussZiel = Join-Path $Inhalt 'data\inventar-ausschluss.toml'
    $tabu = @()
    $ausschlussText = $null
    if (Test-Path -LiteralPath $ausschlussZiel) { $ausschlussText = [IO.File]::ReadAllText($ausschlussZiel) }
    elseif (Test-Path -LiteralPath $ausschlussQuelle) { $ausschlussText = [IO.File]::ReadAllText($ausschlussQuelle) }
    else {
        # Aus der Geschichte: der letzte Stand von inventory.rs mit der Liste im Code.
        $alt = $null
        foreach ($kandidat in @('HEAD') + @((GitAus $Repo @('log', '--format=%H', '-S', 'NEVER_ENTER', '--', 'src/pc/inventory.rs')) -split "`n" | Where-Object { $_ } | ForEach-Object { "$_^"; $_ })) {
            $t = & git -C $Repo show "$($kandidat):src/pc/inventory.rs" 2>$null
            if ($LASTEXITCODE -eq 0 -and (($t -join "`n") -match 'NEVER_ENTER:\s*&\[&str\]\s*=\s*&\[([^\]]*)\]')) { $alt = $Matches[1]; break }
        }
        if (-not $alt) { Abbruch 'Die Ausschlussliste des Inventars fehlt (weder data\inventar-ausschluss.toml noch in der Geschichte von src\pc\inventory.rs). Bitte die Liste von Hand anlegen.' }
        $namen = @([regex]::Matches($alt, '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value })
        $ausschlussText = "# Ordnernamen, die das Inventar nie betritt (Teil des Namens, ohne Groß/Klein).`n# Übernommen beim Umzug des Inhalts aus dem früheren Code ($kurz). Nie leer lassen:`n# Fehlt die Liste oder ist sie leer, zählt das Inventar gar nicht.`nnie_betreten = [" + (($namen | ForEach-Object { '"' + $_ + '"' }) -join ', ') + "]`n"
    }
    $tabu = @([regex]::Matches(([regex]::Match($ausschlussText, '(?ms)nie_betreten\s*=\s*\[(.*?)\]').Groups[1].Value), '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value.ToLowerInvariant() })
    if ($tabu.Count -eq 0) { Abbruch 'Die Ausschlussliste des Inventars ist leer oder kaputt.' }
    function IstTabu([string]$name) { $n = $name.ToLowerInvariant(); foreach ($t in $tabu) { if ($n.Contains($t)) { return $true } } return $false }
    # Ein Pfad mit Tabu-Teil, zum Nennen: nur die Teile davor, der Tabu-Teil als „…“.
    function OhneTabu([string]$rel) {
        $aus = @()
        foreach ($t in ($rel.TrimEnd('/') -split '/')) { if (IstTabu $t) { $aus += '…'; break } else { $aus += $t } }
        $aus -join '/'
    }
    Zeile "Schritt 2: Ausschlussliste mit $($tabu.Count) Namen (Werte stehen nicht hier)."

    # -----------------------------------------------------------------------
    # 3. Was umzieht (eingecheckt und nicht ignoriert). Ein Tabu-Name irgendwo
    #    unter den Orten des Inhalts bricht ab; nichts wird übersprungen.
    # -----------------------------------------------------------------------
    $dateien = New-Object System.Collections.Generic.List[string]
    $tabuFunde = New-Object System.Collections.Generic.List[string]
    $eingecheckt = ((GitAus $Repo (@('ls-files', '-z', '--') + $InhaltZiehtUm)) -split "`0") | Where-Object { $_ -and $_ -notmatch '^projects/_vorlage/' }
    foreach ($d in $eingecheckt) {
        if (@($d -split '/' | Where-Object { IstTabu $_ }).Count -gt 0) { $tabuFunde.Add("eingecheckt: $(OhneTabu $d)"); continue }
        $dateien.Add($d)
    }
    # Nicht eingecheckt, nicht ignoriert: Ordner nur als Ordner (--directory),
    # dann selbst durchlaufen; ein Eintrag mit Tabu-Namen wird nie betreten.
    $neu = ((GitAus $Repo (@('ls-files', '-z', '-o', '--exclude-standard', '--directory', '--') + $InhaltZiehtUm)) -split "`0") | Where-Object { $_ -and $_ -notmatch '^projects/_vorlage/' }
    function Durchlaufen([string]$rel) {
        $voll = Join-Path $Repo (Pfad $rel)
        foreach ($e in Get-ChildItem -LiteralPath $voll -Force) {
            if (IstTabu $e.Name) { $tabuFunde.Add("neu: $(OhneTabu "$rel$($e.Name)")"); continue }
            if ($e.Attributes -band [IO.FileAttributes]::ReparsePoint) { Abbruch "Unter den Orten des Inhalts liegt eine Verknüpfung ($rel$($e.Name)). Bitte erst klären; es wird nichts halb umgezogen." }
            if ($e.PSIsContainer) { Durchlaufen "$rel$($e.Name)/" } else { $dateien.Add("$rel$($e.Name)") }
        }
    }
    foreach ($n in $neu) {
        if (@($n.TrimEnd('/') -split '/' | Where-Object { IstTabu $_ }).Count -gt 0) { $tabuFunde.Add("neu: $(OhneTabu $n)"); continue }
        if ($n.EndsWith('/')) { Durchlaufen $n } else { $dateien.Add($n) }
    }
    # Was git ignoriert: zieht nicht mit um; wird genannt (nie still liegen gelassen).
    $ignoriert = @(((GitAus $Repo (@('ls-files', '-z', '-o', '-i', '--exclude-standard', '--directory', '--') + $InhaltZiehtUm)) -split "`0") | Where-Object { $_ -and $_ -notmatch '^projects/_vorlage/' })
    $ignoriertNennbar = @()
    foreach ($i in $ignoriert) {
        if (@($i.TrimEnd('/') -split '/' | Where-Object { IstTabu $_ }).Count -gt 0) { $tabuFunde.Add("von git ignoriert: $(OhneTabu $i)") } else { $ignoriertNennbar += $i }
    }
    if ($tabuFunde.Count -gt 0) {
        Zeile "  Pfade mit einem Tabu-Namen ($($tabuFunde.Count); der Name selbst steht hier als …):"
        foreach ($f in $tabuFunde) { Zeile "    $f" }
        Abbruch "Unter den Orten des Inhalts trägt ein Pfad einen Tabu-Namen aus der Ausschlussliste. Das Skript öffnet solche Ordner nie, überspringt sie nicht und zieht nichts halb um; es wurde nichts kopiert." @(
            '1. Der Inhaber entscheidet, wohin diese Ordner gehören (sie gehören nicht in projects, data oder docs\entscheidungen).',
            '2. Der Inhaber nimmt sie dort heraus (eingecheckte mit git rm --cached und einem Commit), ohne sie zu öffnen.',
            "3. Dann noch einmal: $(if ($SicherungBestaetigt) { $befehlZweiter } else { $befehlErster })")
    }
    if ($ignoriertNennbar.Count -gt 0) {
        Zeile "  Hinweis: $($ignoriertNennbar.Count) Pfad(e) an den alten Orten ignoriert git. Sie ziehen nicht mit um und bleiben nach dem Merge im Code-Repo liegen (ignoriert):"
        foreach ($i in ($ignoriertNennbar | Select-Object -First 50)) { Zeile "    $i" }
        if ($ignoriertNennbar.Count -gt 50) { Zeile "    … und $($ignoriertNennbar.Count - 50) weitere" }
        Zeile '  Bitte je Pfad entscheiden: von Hand nach dashboard-inhalt holen oder bewusst liegen lassen (Bauordner wie target\ dürfen liegen bleiben).'
    }
    $projekte = @(InhaltAlteProjekte $Repo)
    Zeile "Schritt 3: $($dateien.Count) Dateien ziehen um ($($projekte.Count) Projekte, data, docs\entscheidungen, docs\architektur-stand.md)."

    if ($Pruefen) {
        Zeile "  Stand des Inhalts-Ordners: $(switch ($stand) { 'neu' { 'noch nicht angelegt' } 'halb' { 'halb angelegt (Schritt 3 bis 5)' } 'nach5' { 'nach Schritt 5 (Bau und Einstellungen fehlen)' } 'fertig' { 'erster Durchgang fertig' } })."
        Zeile "  würde kopieren nach $Inhalt, git init, erster Commit als Ranke, dashboard-lokal anlegen, prüfen und bauen."
        Zeile "  im zweiten Durchgang (-SicherungBestaetigt): Zweig $Zweig, der den Inhalt aus dem Code-Repo nimmt."
        Zeile 'Nur geprüft, nichts geändert.'
        exit 0
    }

    $lokal = Join-Path $Inhalt 'dashboard-lokal'
    if (-not $SicherungBestaetigt) {
        if ($stand -eq 'fertig') { Abbruch "$Inhalt ist schon angelegt, der erste Durchgang ist fertig." @('1. Archiv-Rolle: dashboard-inhalt in die Sicherung aufnehmen und bestätigen.', '2. Inhaber als Administrator: Dienst-Einrichten.ps1 -NurLeserecht -Inhalt (Anleitung, „Das Dashboard als Dienst“); dann scripts\Dienst.ps1 -Neustart.', "3. Zweiter Durchgang: $befehlZweiter") }
        if ($stand -eq 'halb') { Abbruch 'Der erste Durchgang ist in Schritt 3 bis 5 stehen geblieben (Kopien ohne fertiges Repo). Das Code-Repo ist unverändert.' (& $umbenennen) }
        if ($stand -eq 'nach5') {
            Zeile 'Der erste Durchgang stand nach Schritt 5 (Repo angelegt, Kisten eingetragen); er macht jetzt bei Schritt 6 weiter.'
        } else {
            # ---------------------------------------------------------------
            # 3b. Kopieren und vergleichen. UMZUG.txt zuerst: So erkennt ein
            #     zweiter Aufruf jeden halben Stand als einen dieses Skripts.
            # ---------------------------------------------------------------
            Schreiben (Join-Path $Inhalt 'UMZUG.txt') ("Umzug des Inhalts aus dem Code-Repo (scripts\Inhalt-Umziehen.ps1): angefangen am $('{0:yyyy-MM-dd HH:mm}' -f (Get-Date)), Stand $quelle.`n")
            $summen = New-Object System.Text.StringBuilder
            foreach ($d in $dateien) {
                $von = Join-Path $Repo (Pfad $d)
                $nach = Join-Path $Inhalt (Pfad $d)
                New-Item -ItemType Directory -Force -Path (Split-Path -Parent $nach) | Out-Null
                Copy-Item -LiteralPath $von -Destination $nach
                $h = Hash $von
                if ((Hash $nach) -ne $h) { Abbruch "Die Kopie von $d stimmt nicht mit dem Original überein." (& $umbenennen) }
                [void]$summen.Append("$h  $d`n")
            }
            Zeile "Schritt 3: $($dateien.Count) Dateien kopiert, jede per SHA-256 gleich."

            # ---------------------------------------------------------------
            # 4. Dateien des Inhalts-Repos
            # ---------------------------------------------------------------
            if (-not (Test-Path -LiteralPath $ausschlussZiel)) { Schreiben $ausschlussZiel $ausschlussText }
            Copy-Item -LiteralPath (Join-Path $Repo '.gitattributes') -Destination (Join-Path $Inhalt '.gitattributes')
            Schreiben (Join-Path $Inhalt '.gitignore') ("# Bauordner (dashboard-lokal baut mit CARGO_TARGET_DIR im Code-Repo)`n/dashboard-lokal/target/`n/projects/*/server/target/`n")
            $ignText = if ($ignoriertNennbar.Count -gt 0) { "`nVon git ignoriert, nicht kopiert (bleibt im Code-Repo liegen):`n" + (($ignoriertNennbar | ForEach-Object { "  $_" }) -join "`n") + "`n" } else { "`nVon git ignoriert: nichts.`n" }
            Schreiben (Join-Path $Inhalt 'UMZUG.txt') ("Umzug des Inhalts aus dem Code-Repo (scripts\Inhalt-Umziehen.ps1).`nStand des Code-Repos: $quelle`nAm: $('{0:yyyy-MM-dd HH:mm}' -f (Get-Date))`nDateien: $($dateien.Count)`n$ignText`nSHA-256 und Pfad je Datei:`n" + $summen.ToString())

            # Namenslisten des Pre-Push-Prüfers (nur hier, nie im Code-Repo).
            $beispiele = @(Get-ChildItem -Path (Join-Path $Repo 'tests') -Directory -Filter 'beispiel*' | ForEach-Object { Join-Path $_.FullName 'projects' } |
                Where-Object { Test-Path -LiteralPath $_ } | ForEach-Object { Get-ChildItem -LiteralPath $_ -Directory | ForEach-Object { $_.Name } } | Sort-Object -Unique)
            $allgemein = @($projekte | Where-Object { $beispiele -notcontains $_ })
            $ohne = @($projekte | Where-Object { $beispiele -contains $_ })
            $schluesselDatei = Join-Path $Inhalt 'data\schluessel.toml'
            $schluessel = @()
            if (Test-Path -LiteralPath $schluesselDatei) {
                $schluessel = @([regex]::Matches([IO.File]::ReadAllText($schluesselDatei), '(?m)^\s*name\s*=\s*"([^"]+)"') | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)
            }
            $liste = { param($v) '[' + (($v | ForEach-Object { '"' + $_ + '"' }) -join ', ') + ']' }
            $namenText = @(
                '# Namenslisten für den Pre-Push-Prüfer des Code-Repos (scripts\Vor-Push.ps1).'
                '# Nur hier, nie im Code-Repo. Groß/Klein egal, Teil eines Wortes zählt;'
                '# Umlaute (ä/ae), Bindestrich, Unterstrich und Leerzeichen gelten gleich.'
                '# Fehlt die Datei, ist sie kaputt oder sind sensibel, allgemein, tabu oder'
                '# benutzer leer (im Basis-Repo auch nur_basis), geht nichts hinaus'
                '# (geschlossen scheitern).'
                ''
                '# In jedem Repo verboten: bürgerliche Namen und ihre Formen, private'
                '# Projektthemen und markante Wortteile davon. Trägt der Inhaber'
                '# ein; leer heißt: nichts geht hinaus.'
                'sensibel = []'
                ''
                '# Nur im Basis-Repo (Code-Repo) verboten, in Projekten und im Inhalt erlaubt:'
                '# Namen wie die Chat-Namen, die keine gewöhnlichen Wörter sind. Trägt der'
                '# Inhaber ein; leer heißt: das Basis-Repo geht nicht hinaus.'
                'nur_basis = []'
                ''
                '# Projektnamen: nur als Pfad projects/<slug> verboten (außer in tests/beispiel*).'
                "# Beim Umzug aus projects\ übernommen; ohne die Namen der Test-Beispiele ($($ohne.Count)), die"
                '# auch im Code-Repo als Beispiele stehen.'
                ('allgemein = ' + (& $liste $allgemein))
                ''
                '# Namen der Tabu-Ordner (aus data\inventar-ausschluss.toml).'
                ('tabu = ' + (& $liste $tabu))
                ''
                '# Windows-Benutzername dieses PCs: überall verboten, auch ohne Pfad.'
                ('benutzer = ' + (& $liste @($env:USERNAME.ToLowerInvariant())))
                ''
                '# Rechnername dieses PCs, WLAN-Namen, MAC-Adressen.'
                ('rechner = ' + (& $liste @($env:COMPUTERNAME.ToLowerInvariant())))
                'wlan = []'
                'mac = []'
                ''
                '# Namen aus data\schluessel.toml (nie Werte).'
                ('schluessel = ' + (& $liste $schluessel))
            ) -join "`n"
            Schreiben (Join-Path $Inhalt 'pruefer\namen.toml') ($namenText + "`n")
            # Art je Remote-Adresse für den Prüfer: leer angelegt, der Inhaber trägt ein.
            $reposDatei = Join-Path $Inhalt 'pruefer\repos.toml'
            if (-not (Test-Path -LiteralPath $reposDatei)) {
                $reposText = @(
                    '# Welche Remote-Adresse welches Repo ist, für den Pre-Push-Prüfer des'
                    '# Code-Repos (scripts\Vor-Push.ps1). Nur hier, nie im geprüften Baum: Was'
                    '# im Baum steht, wählt die Art nie.'
                    '#'
                    '# Je Zeile unter [repos]: "adresse" = "Basis", "Projekt" oder "Inhalt".'
                    '# Die Adresse genau so, wie git sie dem Hook gibt (git remote get-url).'
                    '# Groß/Klein zählt im Pfad, nicht bei Schema und Rechnername; / und .git'
                    '# am Ende zählen nicht. Jede Adresse nur einmal.'
                    '#'
                    '# Basis: das Code-Repo. Dort gelten zusätzlich nur_basis und die'
                    '# Ortsregeln (Positivliste, projects\<slug>, Projektmodule, Binärdateien).'
                    '# Projekt: ein Fork mit projects\<slug>. Inhalt: dashboard-inhalt.'
                    '#'
                    '# Geschlossen scheitern: Eine Adresse, die hier fehlt, eine fehlende Datei'
                    '# und ein Push ohne Adresse prüfen als Basis; eine kaputte Datei hält'
                    '# jeden Push an.'
                    '#'
                    '# Beispiel (erfunden):'
                    '# "https://git.example.invalid/team/projekt-fork.git" = "Projekt"'
                    ''
                    '[repos]'
                ) -join "`n"
                Schreiben $reposDatei ($reposText + "`n")
            }

            # Zusammenbau-Kiste dashboard-lokal aus der Vorlage.
            Copy-Item -LiteralPath (Join-Path $Repo 'vorlagen\dashboard-lokal') -Destination $lokal -Recurse
            $codeRel = ([IO.Path]::GetRelativePath($lokal, $Repo)) -replace '\\', '/'
            $kisten = @()
            foreach ($p in $projekte) {
                $server = Join-Path $Inhalt "projects\$p\server"
                if (-not (Test-Path -LiteralPath (Join-Path $server 'Cargo.toml'))) { continue }
                $cargoDatei = Join-Path $server 'Cargo.toml'
                $sdk = ([IO.Path]::GetRelativePath($server, (Join-Path $Repo 'crates\dashboard-projekt'))) -replace '\\', '/'
                $c = [IO.File]::ReadAllText($cargoDatei).Replace('"../../../crates/dashboard-projekt"', '"' + $sdk + '"')
                [IO.File]::WriteAllText($cargoDatei, $c, $utf8)
                $name = [regex]::Match($c, '(?ms)^\[package\].*?^\s*name\s*=\s*"([^"]+)"').Groups[1].Value
                $typ = $null
                foreach ($rs in Get-ChildItem -LiteralPath (Join-Path $server 'src') -Filter '*.rs' -Recurse) {
                    $m = [regex]::Match([IO.File]::ReadAllText($rs.FullName), 'impl\s+(?:dashboard_projekt::)?Modul\s+for\s+([A-Za-z_][A-Za-z0-9_]*)')
                    if ($m.Success) { $typ = $m.Groups[1].Value; break }
                }
                if (-not $name -or -not $typ) { Abbruch "In projects\$p\server fehlt der Kistenname oder ein Typ mit impl Modul." (& $umbenennen) }
                $kisten += [pscustomobject]@{ Name = $name; Pfad = "../projects/$p/server"; Typ = $typ }
            }
            # Genau wie der Wächter es erwartet (src/projekte/zusammenbau.rs: erwartete_cargo, erwartete_lib).
            $ct = [IO.File]::ReadAllText((Join-Path $lokal 'Cargo.toml')).Replace("`r`n", "`n").Replace('"../../dashboard"', '"' + $codeRel + '"').Replace('"../../dashboard/crates/dashboard-projekt"', '"' + $codeRel + '/crates/dashboard-projekt"')
            $ct = $ct.Replace('# KISTEN-ENDE', ((@($kisten | ForEach-Object { "$($_.Name) = { path = `"$($_.Pfad)`" }" }) + '# KISTEN-ENDE') -join "`n"))
            [IO.File]::WriteAllText((Join-Path $lokal 'Cargo.toml'), $ct, $utf8)
            $lt = [IO.File]::ReadAllText((Join-Path $lokal 'src\lib.rs')).Replace("`r`n", "`n").Replace('        // KISTEN-ENDE', ((@($kisten | ForEach-Object { "        dashboard::eintrag($($_.Name -replace '-', '_')::$($_.Typ))," }) + '        // KISTEN-ENDE') -join "`n"))
            [IO.File]::WriteAllText((Join-Path $lokal 'src\lib.rs'), $lt, $utf8)
            $kt = [IO.File]::ReadAllText((Join-Path $lokal 'kisten.toml')).Replace('pfad = "../../dashboard"', 'pfad = "' + $codeRel + '"')
            [IO.File]::WriteAllText((Join-Path $lokal 'kisten.toml'), $kt, $utf8)
            Copy-Item -LiteralPath (Join-Path $Repo 'Cargo.lock') -Destination (Join-Path $lokal 'Cargo.lock')
            Zeile "Schritt 4: .gitignore, UMZUG.txt, pruefer\namen.toml, pruefer\repos.toml und dashboard-lokal mit $($kisten.Count) Kiste(n) angelegt."

            # ---------------------------------------------------------------
            # 5. Repo anlegen, erster Commit als Ranke, dann der geprüfte Commit
            # ---------------------------------------------------------------
            GitAus $Inhalt @('init', '-q', '-b', 'main') | Out-Null
            GitAus $Inhalt @('config', 'user.name', 'Ranke') | Out-Null
            GitAus $Inhalt @('config', 'user.email', 'ranke2@proton.me') | Out-Null
            GitAus $Inhalt @('add', '-A') | Out-Null
            GitRanke $Inhalt @('commit', '-q', '-m', "Inhalt aus dem Code-Repo übernommen (Stand $kurz)`n`nProjekte, data, docs/entscheidungen, docs/architektur-stand.md, Ausschlussliste,`nNamenslisten des Prüfers und die Zusammenbau-Kiste dashboard-lokal.") | Out-Null
            $erster = (GitAus $Inhalt @('rev-parse', '--short=12', 'HEAD')).Trim()
            $eintraege = @($kisten | ForEach-Object { "`n[[kiste]]`nname = `"$($_.Name)`"`npfad = `"$($_.Pfad)`"`ntyp = `"$($_.Typ)`"`ncommit = `"$erster`"`n" }) -join ''
            [IO.File]::WriteAllText((Join-Path $lokal 'kisten.toml'), $kt + $eintraege, $utf8)
            GitAus $Inhalt @('add', '-A') | Out-Null
            GitRanke $Inhalt @('commit', '-q', '--allow-empty', '-m', "dashboard-lokal: geprüfte Kisten mit Commit $erster") | Out-Null
            Zeile "Schritt 5: Repo angelegt; Commit $erster (Inhalt), Kisten mit diesem Stand eingetragen."
        }

        # -------------------------------------------------------------------
        # 6. dashboard-lokal: Wächter aus dem Code-Repo vor jedem cargo-Aufruf
        #    dort, dann bauen (--locked). Bleibt es hier stehen, macht ein
        #    zweiter Aufruf hier weiter.
        # -------------------------------------------------------------------
        $weiter6 = @('1. Die Ursache oben beheben (der Inhalt liegt kopiert und committet da; das Code-Repo ist unverändert).', "2. Denselben Befehl noch einmal; er macht bei Schritt 6 weiter: $befehlErster")
        if ($OhneBau) {
            Zeile 'Schritt 6: übersprungen (-OhneBau).'
        } else {
            $altZiel = $env:CARGO_TARGET_DIR
            $env:CARGO_TARGET_DIR = $Bauordner
            $dashExe = Join-Path $Bauordner 'release\dashboard.exe'
            try {
                $ErrorActionPreference = 'Continue'
                Push-Location $Repo
                try { & cargo build --release --locked --offline 2>&1 | Select-Object -Last 2 | ForEach-Object { Zeile "    $_" }; $code = $LASTEXITCODE } finally { Pop-Location }
                if ($code -ne 0) { $ErrorActionPreference = 'Stop'; Abbruch 'Der Bau des Code-Repos (für den Wächter) ist gescheitert.' $weiter6 }
                # Ohne Wächter kein cargo in dashboard-lokal (sonst stammte $LASTEXITCODE noch vom Bau davor).
                if (-not (Test-Path -LiteralPath $dashExe -PathType Leaf)) { $ErrorActionPreference = 'Stop'; $global:LASTEXITCODE = 1; Abbruch "Der Wächter fehlt: $dashExe gibt es nicht (hat cargo build es nicht geschrieben?). In dashboard-lokal lief kein cargo." $weiter6 }
                Zeile '  Wächter (ohne Cargo.lock), vor jedem cargo-Aufruf in dashboard-lokal:'
                & $dashExe pruefe-zusammenbau $lokal --code $Repo --ohne-lock 2>&1 | ForEach-Object { Zeile "    $_" }
                if ($LASTEXITCODE -ne 0) { $ErrorActionPreference = 'Stop'; Abbruch 'Der Wächter der Zusammenbau-Kiste ist rot; in dashboard-lokal lief kein cargo.' $weiter6 }
                Push-Location $lokal
                try { & cargo --config (Join-Path $Repo '.cargo\config.toml') update --workspace --offline 2>&1 | Select-Object -Last 3 | ForEach-Object { Zeile "    $_" }; $code = $LASTEXITCODE } finally { Pop-Location }
                if ($code -ne 0) { $ErrorActionPreference = 'Stop'; Abbruch 'cargo update --workspace (schreibt nur Cargo.lock) ist gescheitert.' $weiter6 }
                Zeile '  Wächter (ganz, mit Cargo.lock):'
                & $dashExe pruefe-zusammenbau $lokal --code $Repo 2>&1 | ForEach-Object { Zeile "    $_" }
                if ($LASTEXITCODE -ne 0) { $ErrorActionPreference = 'Stop'; Abbruch 'Der Wächter der Zusammenbau-Kiste ist rot; dashboard-lokal wurde nicht gebaut.' $weiter6 }
                Push-Location $lokal
                try { & cargo --config (Join-Path $Repo '.cargo\config.toml') build --release --locked --offline 2>&1 | Select-Object -Last 2 | ForEach-Object { Zeile "    $_" }; $code = $LASTEXITCODE } finally { Pop-Location }
                $ErrorActionPreference = 'Stop'
                if ($code -ne 0) { Abbruch 'Der Bau von dashboard-lokal ist gescheitert (der Wächter war grün).' $weiter6 }
            } finally { $env:CARGO_TARGET_DIR = $altZiel; $ErrorActionPreference = 'Stop' }
            if ((GitAus $Inhalt @('status', '--porcelain')).Trim()) {
                GitAus $Inhalt @('add', '-A') | Out-Null
                GitRanke $Inhalt @('commit', '-q', '-m', 'dashboard-lokal: Cargo.lock nach dem ersten Bau') | Out-Null
            }
            Zeile 'Schritt 6: dashboard-lokal geprüft (Wächter grün) und gebaut.'
        }

        # -------------------------------------------------------------------
        # 7. Einstellungen, Merker „fertig“ und Befehle für den Inhaber
        # -------------------------------------------------------------------
        $abschnitt = "[inhalt]`nprojekte = [`"$((Join-Path $Inhalt 'projects') -replace '\\', '/')`"]`ndaten = `"$((Join-Path $Inhalt 'data') -replace '\\', '/')`"`nentscheidungen = `"$((Join-Path $Inhalt 'docs\entscheidungen') -replace '\\', '/')`"`n"
        if ($DienstToml) {
            $t = [IO.File]::ReadAllText($DienstToml)
            if ($t -match '(?m)^\s*\[inhalt\]') { Zeile "Schritt 7: $DienstToml hat schon [inhalt]; unverändert." }
            else { [IO.File]::WriteAllText($DienstToml, $t.TrimEnd() + "`n`n" + $abschnitt, $utf8); Zeile "Schritt 7: [inhalt] in $DienstToml geschrieben." }
        } else {
            Zeile 'Schritt 7: Die Einstellungen unter ProgramData schreibt nur ein Administrator. Du, einmal als Administrator:'
            Zeile "    C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$Repo\scripts\Dienst-Einrichten.ps1`" -NurLeserecht -Inhalt `"$Inhalt`""
            Zeile '  Das hängt [inhalt] an dienst.toml an und gibt dem Dienstkonto Lese-Recht auf projects, data und docs\entscheidungen im Inhalt.'
        }
        [IO.File]::AppendAllText((Join-Path $Inhalt 'UMZUG.txt'), "`nErster Durchgang fertig: $('{0:yyyy-MM-dd HH:mm}' -f (Get-Date))$(if ($OhneBau) { ' (ohne Bau, -OhneBau)' })`n", $utf8)
        GitAus $Inhalt @('add', 'UMZUG.txt') | Out-Null
        GitRanke $Inhalt @('commit', '-q', '-m', 'Umzug: erster Durchgang fertig') | Out-Null
        Zeile ''
        Zeile 'Erster Durchgang fertig. Als Nächstes:'
        Zeile '  1. Archiv-Rolle: dashboard-inhalt in die Sicherung aufnehmen und bestätigen.'
        Zeile '  2. Inhaber als Administrator: Dienst-Einrichten.ps1 -NurLeserecht -Inhalt (siehe oben).'
        Zeile '  3. scripts\Dienst.ps1 -Neustart (Wächter, dann Bau von dashboard-lokal; liest [inhalt]).'
        Zeile "  4. Zweiter Durchgang: $befehlZweiter"
        exit 0
    }

    # -----------------------------------------------------------------------
    # 8. Zweiter Durchgang. Erst: [inhalt] steht in dienst.toml, und der
    #    Dienst liest wirklich aus dem Inhalt (/api/betrieb/voll, quellen).
    # -----------------------------------------------------------------------
    $weiterDienst = @('1. Inhaber als Administrator: Dienst-Einrichten.ps1 -NurLeserecht -Inhalt (Anleitung, „Das Dashboard als Dienst“).', '2. scripts\Dienst.ps1 -Neustart, dann scripts\Dienst.ps1 -Status.', "3. Noch einmal: $befehlZweiter")
    $tomlDatei = if ($DienstToml) { $DienstToml } else { 'C:\ProgramData\Dashboard\dienst.toml' }
    $tomlText = ''
    try { $tomlText = [IO.File]::ReadAllText($tomlDatei) } catch { Abbruch "$tomlDatei lässt sich nicht lesen ($($_.Exception.Message))." $weiterDienst }
    $sollProjekte = Join-Path $Inhalt 'projects'
    $inhaltAbschnitt = [regex]::Match($tomlText, '(?ms)^\s*\[inhalt\]\s*$(.*?)(?=^\s*\[|\z)')
    if (-not $inhaltAbschnitt.Success -or -not ($inhaltAbschnitt.Groups[1].Value.Replace('\\', '/').Replace('\', '/') -match [regex]::Escape(($sollProjekte -replace '\\', '/')))) {
        Abbruch "In $tomlDatei steht kein Abschnitt [inhalt] mit $sollProjekte." $weiterDienst
    }
    if (-not $Server) {
        $port = [regex]::Match($tomlText, '(?m)^\s*port\s*=\s*(\d+)')
        $Server = "127.0.0.1:$(if ($port.Success) { $port.Groups[1].Value } else { '8787' })"
    }
    $ErrorActionPreference = 'Continue'
    $antwort = (& (Join-Path $PSScriptRoot 'Api.ps1') 'api/betrieb/voll' -Server $Server 2>&1 | ForEach-Object { "$_" }) -join "`n"
    $apiCode = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($apiCode -ne 0) { Abbruch "Der Dienst auf $Server antwortet auf /api/betrieb/voll nicht ($($antwort.Trim()))." $weiterDienst }
    $b = $null
    try { $b = $antwort | ConvertFrom-Json } catch { Abbruch "/api/betrieb/voll lieferte kein JSON." $weiterDienst }
    function Voll([string]$p) { ([Environment]::ExpandEnvironmentVariables($p) -replace '/', '\').TrimEnd('\') }
    $q = $b.quellen
    $liestInhalt = $q -and $q.inhalt_abschnitt -and
        (@($q.projekte | Where-Object { (Voll $_) -ieq $sollProjekte }).Count -gt 0) -and
        ((Voll $q.daten) -ieq (Join-Path $Inhalt 'data')) -and
        ((Voll $q.entscheidungen) -ieq (Join-Path $Inhalt 'docs\entscheidungen'))
    if (-not $liestInhalt) {
        $wo = if ($q) { "Quellen: $(@($q.projekte) -join ', '); Daten: $($q.daten); Entscheidungen: $($q.entscheidungen); [inhalt]: $($q.inhalt_abschnitt)" } else { 'die Fassung des Dienstes kennt das Feld quellen noch nicht' }
        Abbruch "Der Dienst liest noch nicht aus $Inhalt ($wo)." $weiterDienst
    }
    # Jeder Ordner, den der Dienst liest, muss „lesbar“ melden (nicht nur der Pfad stimmen).
    $nichtLesbar = @($b.ordner | Where-Object { $_.stand -ne 'lesbar' })
    if (@($b.ordner).Count -eq 0 -or $nichtLesbar.Count -gt 0) {
        $wo = if (@($b.ordner).Count -eq 0) { 'der Dienst nennt keine Ordner' } else { ($nichtLesbar | ForEach-Object { "$($_.name): $($_.stand)" }) -join ', ' }
        Abbruch "Der Dienst kann nicht alle Ordner lesen ($wo)." $weiterDienst
    }
    Zeile "Schritt 8: dienst.toml nennt [inhalt], und der Dienst auf $Server liest aus $Inhalt; alle $(@($b.ordner).Count) Ordner melden lesbar."

    # Maßstab ist der erste Commit im Inhalt (dort darf seitdem weitergearbeitet
    # worden sein): Jede Datei, die im Code-Repo noch liegt, muss mit derselben
    # Prüfsumme in UMZUG.txt stehen und im ersten Commit des Inhalts vorkommen.
    $manifest = @{}
    foreach ($z in ((GitAus $Inhalt @('show', 'HEAD:UMZUG.txt')) -split "`n")) {
        if ($z -match '^([0-9A-F]{64})  (.+)$') { $manifest[$Matches[2].TrimEnd("`r")] = $Matches[1] }
    }
    $erster = (GitAus $Inhalt @('rev-list', '--max-parents=0', 'HEAD')).Trim()
    $imErsten = @{}
    foreach ($p in ((GitAus $Inhalt @('ls-tree', '-r', '-z', '--name-only', $erster)) -split "`0")) { if ($p) { $imErsten[$p] = $true } }
    $fehlt = 0
    foreach ($d in $dateien) {
        if (-not $manifest.ContainsKey($d) -or $manifest[$d] -ne (Hash (Join-Path $Repo (Pfad $d))) -or -not $imErsten.ContainsKey($d)) {
            $fehlt++
            Zeile "  seit dem ersten Durchgang neu oder geändert: $d"
        }
    }
    if ($fehlt -gt 0) { Abbruch "$fehlt Datei(en) im Code-Repo sind seit dem Kopieren neu oder geändert." @('1. Sie in dashboard-inhalt nachziehen und dort committen (als Ranke).', '2. Der Inhaber entscheidet mit der Architektur-Rolle, ob der erste Commit neu entsteht (dann den Ordner umbenennen und den ersten Durchgang neu).', "3. Sonst noch einmal: $befehlZweiter") }
    Zeile "Schritt 8: alle $($dateien.Count) Dateien liegen unverändert im ersten Commit des Inhalts (SHA-256 wie beim Kopieren)."

    # -----------------------------------------------------------------------
    # 9. Zweig, der den Inhalt aus dem Code-Repo nimmt (eigener Worktree)
    # -----------------------------------------------------------------------
    if (& git -C $Repo rev-parse --verify --quiet "refs/heads/$Zweig") {
        Abbruch "Den Zweig $Zweig gibt es schon." @("Ein früherer zweiter Durchgang war fertig: Der Zweig wartet auf Abnahme und Merge (git -C `"$Repo`" log --oneline main..$Zweig). Sonst nichts zu tun.")
    }
    # Der Worktree liegt kurz neben dem Code-Repo (kurze Pfade; web\vendor hat
    # lange Dateinamen) und verschwindet am Ende wieder.
    $kopf = (GitAus $Repo @('rev-parse', 'HEAD')).Trim()
    $wt = Join-Path (Split-Path -Parent $Repo) ("umzug-" + [Guid]::NewGuid().ToString('N').Substring(0, 6))
    $neuerStand = $null
    try {
        GitAus $Repo @('-c', 'core.longpaths=true', 'worktree', 'add', '-q', '-b', $Zweig, $wt, 'HEAD') | Out-Null
        $raus = @($projekte | ForEach-Object { "projects/$_" }) + @($InhaltRaus | Where-Object { (GitAus $wt @('ls-files', '--', $_)).Trim() })
        GitAus $wt (@('rm', '-r', '-q', '--') + $raus) | Out-Null
        $gi = Join-Path $wt '.gitignore'
        $alt = [IO.File]::ReadAllText($gi)
        [IO.File]::WriteAllText($gi, $alt.TrimEnd() + "`n" + ($InhaltIgnorieren -join "`n") + "`n", $utf8)
        GitAus $wt @('add', '.gitignore') | Out-Null
        GitRanke $wt @('commit', '-q', '-m', "Umzug des Inhalts: privater Inhalt liegt jetzt in dashboard-inhalt`n`nEntfernt: projects/* ausser _vorlage, data/, docs/entscheidungen/,`ndocs/architektur-stand.md, web/src/lib/vertrag/projekte/. Kopie geprüft (SHA-256),`nSicherung bestätigt. Stand vorher: $kurz.") | Out-Null
        $rest = @(((GitAus $wt @('ls-files', '-z', '--', 'projects', 'data', 'docs/entscheidungen', 'docs/architektur-stand.md', 'web/src/lib/vertrag/projekte')) -split "`0") | Where-Object { $_ -and $_ -notmatch '^projects/_vorlage/' })
        if ($rest.Count -gt 0) { throw "Nach dem Entfernen ist noch Inhalt eingecheckt ($($rest.Count) Dateien)." }
        $neuerStand = (GitAus $wt @('rev-parse', '--short=12', 'HEAD')).Trim()
    } finally {
        & git -C $Repo -c core.longpaths=true worktree remove --force $wt 2>$null | Out-Null
        & git -C $Repo worktree prune 2>$null | Out-Null
        if (Test-Path -LiteralPath $wt) { Zeile "  Hinweis: Der Hilfs-Worktree $wt blieb liegen (nur eine Kopie aus git); bitte mit git worktree remove --force entfernen." }
        # Halb angelegt: Der neue Zweig zeigt nur auf HEAD (kein Commit darauf) und geht wieder weg.
        if (-not $neuerStand -and (((& git -C $Repo rev-parse --verify --quiet "refs/heads/$Zweig") | Out-String).Trim() -eq $kopf)) {
            & git -C $Repo branch -q -D $Zweig 2>$null | Out-Null
        }
    }
    Zeile "Schritt 9: Zweig $Zweig ($neuerStand) nimmt den Inhalt aus dem Code-Repo. Der Arbeitsbaum ist unverändert."
    if ($ignoriertNennbar.Count -gt 0) { Zeile "  Erinnerung: $($ignoriertNennbar.Count) ignorierte Pfad(e) an den alten Orten (oben genannt) bleiben nach dem Merge liegen." }
    Zeile ''
    Zeile 'Zweiter Durchgang fertig. Als Nächstes (Inhaber, nach der Abnahme):'
    Zeile "  git merge --no-ff $Zweig   (aus dem Arbeitsbaum verschwinden die Dateien erst jetzt)"
    Zeile '  scripts\Dienst.ps1 -Neustart'
    exit 0
} catch {
    Write-Host "FEHLER $($_.Exception.Message) Es wurde nichts entfernt."
    $s = $null
    try { $s = Stand } catch { }
    if ($SicherungBestaetigt) { Write-Host "So geht es weiter: die Ursache beheben, dann noch einmal: $befehlZweiter (ein halb angelegter Zweig ist schon wieder weg)." }
    elseif ($s -eq 'nach5') { Write-Host "So geht es weiter: die Ursache beheben, dann noch einmal (macht bei Schritt 6 weiter): $befehlErster" }
    elseif ($s -eq 'halb') { Write-Host 'So geht es weiter:'; foreach ($w in (& $umbenennen)) { Write-Host "  $w" } }
    exit 2
}
