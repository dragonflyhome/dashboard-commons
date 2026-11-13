<#
.SYNOPSIS
    Richtet das Dashboard als Windows-Dienst ein (einmal, als Administrator).
.DESCRIPTION
    Prüft zuerst alles und zeigt, was gemacht wird. Mit -Pruefen endet es dort,
    ohne etwas zu ändern (das geht auch ohne Administrator).

    Sonst, in dieser Reihenfolge:
      1. Probe mit einem Testdienst <Dienstname>-Test auf Port 8788 in einem
         eigenen Ordner C:\ProgramData\<Dienstname>-Test. Er wird eingerichtet,
         gestartet, auf 200 geprüft und wieder ganz entfernt. Nur wenn die Probe
         gelingt, geht es weiter; sonst wird nichts Echtes angelegt.
      2. Ein laufendes Dashboard auf dem Port beenden (nur den Prozess
         dashboard.exe, der dort lauscht; nie nach Namen).
      3. Dienst anlegen (noch ohne Start), damit es das Konto NT SERVICE\<Dienstname>
         gibt. Gleich danach die Prozessrechte des Dienstes auf
         SeChangeNotifyPrivilege beschränken (sc privs, geprüft mit sc qprivs);
         vor allem fehlt dann SeImpersonatePrivilege. Du darfst bin ändern;
         mit SeImpersonate könnte ein dort getauschtes Programm auf bekannten
         Wegen zu SYSTEM werden. Dasselbe gilt für den Testdienst.
         Dann Ordner C:\ProgramData\<Dienstname> mit bin, daten, sicherung,
         protokoll und protokoll\bote und ihre Rechte. In protokoll\bote
         schreibt der Bote (unter deinem Konto) sein Protokoll; dort dürfen dein Konto und
         das Dienstkonto ändern (vererbt; der Dienst räumt alte Dateien weg).
         Unter ProgramData darf jedes Konto Ordner anlegen. Ein schon
         vorhandener Ordner wird nur genommen, wenn er den Administratoren
         (oder SYSTEM) gehört und weder er noch etwas darin ein Reparse-Punkt
         (Verknüpfung, Junction) ist; sonst Abbruch mit Satz. Ein neuer Ordner
         wird sofort gesperrt (Besitzer Administratoren, Vererbung aus) und
         muss dann noch leer sein. Fremde ausdrückliche Einträge werden
         entfernt.
      4. Programme aus %LOCALAPPDATA%\Dashboard\cache\dienst\release (dem Bauordner
         von Dienst.ps1 -Neustart; gibt es release\zusammenbau, von dort) nach bin
         (die vorige Fassung bleibt als *.alt.exe). Dazu die Oberfläche, die
         Dienst.ps1 -Neustart als web neben die Programme legt (fehlt sie,
         web\build im Repo), nach bin\web, wenn bin\web einen anderen Grundbau
         hat (_app\version.json); ein Bau ohne diese Kennung wird nicht eingespielt.
      5. Datenspeicher und Sicherungen aus %USERPROFILE%\.var\dashboard kopieren,
         nur wenn unter ProgramData noch keiner liegt und weder .var,
         .var\dashboard noch etwas darin ein Reparse-Punkt ist. Das Original
         bleibt liegen.
      6. dienst.toml schreiben (nur wenn sie fehlt; sonst wird sie geprüft und
         dienst_sid nachgetragen, falls sie fehlt). Unter [konten] stehen
         inhaber_sid (dein Konto) und dienst_sid (NT SERVICE\<Dienstname>, aus
         sc.exe showsid); der Bote braucht dienst_sid für die Rechte an der Pipe.
      7. Lese-Recht für das Dienstkonto im Repo, nur wo es nötig ist:
         <repo>\projects, <repo>\data und <repo>\web lesen und ausführen
         (vererbt; web ganz, damit jeder Neubau der Oberfläche das Recht erbt,
         web\node_modules damit nur lesbar), <repo> selbst nur lesen (ohne
         Vererbung, damit der Ordner sich öffnen lässt). Dazu (seit 4d)
         <repo>\docs\entscheidungen und <repo>\docs\zielbild (seit 6b auch
         <repo>\docs\anleitung) lesen und ausführen (vererbt), nur diese,
         nicht ganz docs; fehlt einer
         davon, wird er übersprungen (eine Zeile sagt es), nicht angelegt.
         target, .git und alles andere im Repo und im Profil bleiben außen
         vor. Eine ältere Regel auf dem ganzen Repo (vererbt) und eine eigene
         auf web\build werden dabei entfernt.
      8. Dienst einstellen: Neustart nach Fehlern, Rechte für dein Konto
         (abfragen, starten, stoppen; nicht umkonfigurieren, nicht löschen).
      9. Aufgabe für den Boten in der Aufgabenplanung (startet bei deiner Anmeldung
         bin\dashboard-bote.exe --waechter; der Wächter startet den Boten und
         nach einem Fehler neu).
     10. Dienst und Boten starten, / und /api/daten (mit Windows-Anmeldung)
         auf 200 prüfen, danach
         Start beim Hochfahren einschalten. LIESMICH.txt liegt dann im Ordner.

    Die Probe prüft außer / und /healthz, dass /api/anmeldung/wer
    verfuegbar: true meldet, die Windows-Anmeldung (Negotiate) nicht mit 500
    scheitert und /api/betrieb einen Boten vorsieht (frei nur "vorgesehen";
    ob er läuft, steht seit der Kopfleiste 2 nur in /api/betrieb/voll):
    Anmeldung und Pipe gehen also
    ohne SeImpersonate. Ein Testordner wird nur gelöscht, wenn dieser Lauf
    ihn angelegt hat und nichts darin eine Verknüpfung ist.

    Administrator-Sitzung: Start am besten ohne Profil, mit vollem Pfad:
      C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile
        -NoExit -ExecutionPolicy Bypass -File <Repo>\scripts\Dienst-Einrichten.ps1
    (Windows-Taste+R, eintippen, Strg+Umschalt+Enter startet als
    Administrator). Im erhöhten Lauf setzt das Skript PSModulePath auf die
    Systempfade, lädt ScheduledTasks und NetTCPIP mit vollem Pfad, arbeitet
    in System32 und ruft sc.exe, icacls.exe und curl.exe mit vollem Pfad
    auf. System32, ProgramData und Programme kommen von Windows selbst
    (GetFolderPath), nicht aus Umgebungsvariablen, die das Konto setzen kann.
    Wurde ein Profil geladen, warnt es.

    Nur das Lese-Recht nachziehen (bestehende Installation, etwa nach 4d für
    docs\entscheidungen und docs\zielbild, nach 6b für docs\anleitung):
    einmal als Administrator, wie
    oben ohne Profil, aus dem Haupt-Repo auf main:
      C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile
        -NoExit -ExecutionPolicy Bypass -File <Repo>\scripts\Dienst-Einrichten.ps1 -NurLeserecht
    Das setzt nur Schritt 7 für den schon eingerichteten Dienst: keine Probe,
    kein Anhalten, keine Programme, nichts unter ProgramData. Der Dienst
    liest die Ordner danach sofort, ein Neustart ist nicht nötig. Mit
    -Pruefen dazu zeigt es nur, was fehlt.

    Jeder Schritt prüft zuerst, ob er schon getan ist; ein zweiter Lauf repariert,
    statt doppelt anzulegen. Jeder Schritt schreibt eine Zeile, auch nach
    C:\ProgramData\<Dienstname>\protokoll\einrichten-JJJJ-MM-TT.log.
    Scheitert ein Schritt, bricht das Skript mit einem Satz ab;
    scripts\Dienst-Entfernen.ps1 nimmt dann alles zurück.

    Rechte am Dienst (sc sdset): Dein Konto bekommt den Eintrag
    (A;;CCLCSWRPWPLORC;;;<SID>), nur Abfragen und Start/Stopp. Das heißt:
    CC Einstellungen lesen, LC Zustand abfragen, SW abhängige Dienste
    auflisten, RP starten, WP stoppen, LO nachfragen (Interrogate),
    RC Rechte lesen. Nicht dabei: DT (anhalten und fortsetzen), CR (eigene
    Steuerbefehle), DC (umkonfigurieren), SD (löschen), WD und WO (Rechte
    oder Besitzer ändern). Ein älterer Eintrag für dein Konto wird ersetzt.

    Das Skript gehört in das Haupt-Repo auf dem Branch main. Läuft es aus
    einem Git-Worktree oder von einem anderen Branch, warnt es; ein echter
    Lauf bricht dann ab (außer mit -Trotzdem), -Pruefen nur warnt.

    Exit-Codes:
      0  fertig (bei -Pruefen: alle Prüfungen gut, nichts geändert)
      1  eine Prüfung oder ein Schritt ist gescheitert (der Satz nennt ihn)
      5  ohne -Pruefen braucht das Skript Administrator-Rechte
.PARAMETER Pruefen
    Nur prüfen und zeigen, was gemacht würde. Ändert nichts, braucht keinen Administrator.
.PARAMETER Konto
    Dein Windows-Konto (etwa PC\name). Standard: das Konto, das das Skript startet.
    Nötig, wenn die UAC-Abfrage mit einem eigenen Administrator-Konto bestätigt wird.
.PARAMETER Port
    Port des Dienstes (Standard 8787).
.PARAMETER OhneTest
    Die Probe mit dem Testdienst überspringen.
.PARAMETER Dienstname
    Name des Dienstes (Standard Dashboard).
.PARAMETER TestPort
    Port des Testdienstes (Standard 8788; muss frei sein).
.PARAMETER Durchlaufrecht
    Gibt dem Dienstkonto auf den Ordnern über dem Repo (Profil, dev) nur das
    Durchlaufrecht (X, ohne Inhalt, ohne Vererbung). Normalerweise nicht nötig,
    weil Windows jedem Konto erlaubt, durch Ordner hindurchzugehen
    ("Auslassen der durchsuchenden Überprüfung"). Nur nehmen, wenn die Probe
    sagt, dass / nicht antwortet.
.PARAMETER Trotzdem
    Auch aus einem Git-Worktree oder von einem anderen Branch als main
    einrichten (sonst Abbruch mit Exit 1).
.PARAMETER NurLeserecht
    Nur das Lese-Recht des schon eingerichteten Dienstkontos im Repo setzen
    (Schritt 7: projects, data, web, docs\entscheidungen, docs\zielbild,
    docs\anleitung und der Repo-Ordner selbst). Sonst ändert es nichts. Mit
    -Pruefen nur zeigen.
    Steht in dienst.toml ein Abschnitt [inhalt] (Umzug des Inhalts), gilt
    das Lese-Recht statt für projects, data und docs\entscheidungen im Repo
    für die Ordner von dort: jede Quelle aus projekte, daten und
    entscheidungen (wenn es sie gibt). web, docs\zielbild und docs\anleitung
    bleiben im Repo.
.PARAMETER Inhalt
    Nur mit -NurLeserecht oder beim Einrichten: der Ordner des privaten
    Inhalts (etwa %USERPROFILE%\dev\dashboard-inhalt, mit projects, data und
    docs\entscheidungen). Fehlt in dienst.toml der Abschnitt [inhalt], wird er
    mit diesen drei Ordnern angehängt (sonst nichts an der Datei geändert);
    steht schon einer da, muss er auf denselben Ordner zeigen. Danach gilt das
    Lese-Recht für diese Ordner. Ein Neustart des Dienstes
    (scripts\Dienst.ps1 -Neustart) liest die neuen Ordner. Der Ordner (und
    jeder Ordner aus [inhalt]) muss ein vollständiger Pfad mit Laufwerk sein,
    ohne . und .., im Profil deines Kontos, nicht das Profil selbst, keine
    Laufwerkswurzel, nicht das Code-Repo und nicht darin oder darüber, und auf
    dem Weg dorthin darf keine Verknüpfung liegen (Inhalt-Unsicher in
    Inhalt-Orte.ps1); sonst bricht es ab, ohne etwas zu ändern.
.PARAMETER NurGeheim
    Nur den Ordner C:\ProgramData\<Dienstname>\geheim für den Schlüssel der
    Passkey-Ausweise (Etappe 7c) anlegen bzw. seine Rechte setzen: Vererbung
    aus, SYSTEM und Administratoren voll, NT SERVICE\<Dienstname> ändern,
    sonst niemand. Für einen Dienst, der vor 7c eingerichtet wurde. Den
    Schlüssel selbst legt der Dienst beim nächsten Start an (danach
    scripts\Dienst.ps1 -Neustart). Sonst ändert es nichts. Mit -Pruefen nur zeigen.
#>
[CmdletBinding()]
param(
    [switch]$Pruefen,
    [string]$Konto,
    [ValidateRange(1, 65535)][int]$Port = 8787,
    [switch]$OhneTest,
    [ValidatePattern('^[A-Za-z][A-Za-z0-9-]{0,39}$')][string]$Dienstname = 'Dashboard',
    [ValidateRange(1, 65535)][int]$TestPort = 8788,
    [switch]$Durchlaufrecht,
    [switch]$Trotzdem,
    [switch]$NurLeserecht,
    [switch]$NurGeheim,
    [string]$Inhalt
)

$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
# Inhalt-Unsicher (Prüfung des Inhalts-Ordners), dieselbe wie in Inhalt-Umziehen.ps1.
. (Join-Path $PSScriptRoot 'Inhalt-Orte.ps1')
# Systemordner von Windows selbst, nicht aus Umgebungsvariablen: SystemRoot,
# ProgramData und ProgramFiles lassen sich als Benutzer-Variable (HKCU\Environment)
# überschreiben, und die gilt auch im erhöhten Lauf desselben Kontos.
$sys32 = [Environment]::GetFolderPath([Environment+SpecialFolder]::System)
$programData = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
$programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
$scPfad = Join-Path $sys32 'sc.exe'
$icaclsPfad = Join-Path $sys32 'icacls.exe'
$curlPfad = Join-Path $sys32 'curl.exe'
# Programme aus dem Bauordner von Dienst.ps1 -Neustart, nie aus target des Repos
# (dort liegt, was zuletzt von Hand gebaut wurde, auch ein Zweig).
$quelle = Join-Path $env:LOCALAPPDATA 'Dashboard\cache\dienst\release'
if (Test-Path -LiteralPath (Join-Path $quelle 'zusammenbau\dashboard.exe')) { $quelle = Join-Path $quelle 'zusammenbau' }
$programme = @('dashboard.exe', 'dashboard-bote.exe', 'dashboard-herzschlag.exe')
# Die Oberfläche zu diesen Programmen: die Kopie, die Dienst.ps1 -Neustart daneben
# ablegt (derselbe Lauf), nicht web\build im Repo, das seitdem neuer sein kann.
# shortcut: ohne sie (erstes Einrichten nach einem cargo build von Hand) web\build im Repo, nur mit Kennung; aufrüsten, wenn das erste Einrichten über Fassung-Bauen läuft.
$quelleWeb = Join-Path $quelle 'web'
if (-not (Test-Path -LiteralPath (Join-Path $quelleWeb 'index.html') -PathType Leaf)) { $quelleWeb = Join-Path $repo 'web\build' }
$wurzel = Join-Path $programData $Dienstname
$tomlPfad = Join-Path $wurzel 'dienst.toml'
# Aus welcher dienst.toml InhaltOrdner liest (beim Testdienst dessen Datei).
$script:tomlFuerInhalt = $tomlPfad
$aufgabenOrdner = "\$Dienstname\"
$aufgabenName = "$Dienstname-Bote"
$pipeName = "$($Dienstname.ToLower())-bote"
$liesmichQuelle = Join-Path $PSScriptRoot 'LIESMICH-Dienst.txt'
$sidSystem = 'S-1-5-18'
$sidAdmins = 'S-1-5-32-544'
# Prozessrechte des Dienstes (sc privs): nur SeChangeNotifyPrivilege
# (Ordner durchqueren, ohne jeden Ordner darüber lesen zu dürfen; ohne es
# scheitern viele Dateizugriffe). Bewusst ohne SeImpersonatePrivilege (damit
# würde ein getauschtes Programm in bin über bekannte Wege SYSTEM) und ohne
# SeCreateGlobalPrivilege: Das braucht nur, wer außerhalb von Sitzung 0
# Dateizuordnungen oder symbolische Links im Namensraum Global\ anlegt. Der
# Dienst läuft in Sitzung 0 und legt keine solchen Objekte an; benannte
# Pipes (der Bote) brauchen es nicht, und der Dienst ist dort nur Client.
$dienstPrivilegien = @('SeChangeNotifyPrivilege')
# Ordner im Repo, die der Dienst lesen muss (vererbt). Sonst nichts im Repo.
# web statt nur web\build: npm run build (und später der Baumeister mit
# web\builds\<n>) legt den Ausgabeordner neu an; unter web erbt er das Recht.
# projects, data und docs\entscheidungen kommen seit dem Umzug des Inhalts
# aus InhaltOrdner (aus [inhalt] in dienst.toml, sonst wie bisher im Repo).
$leseOrdner = @('web')
# Seit 4d liest der Server auch die System-Seiten aus docs (Entscheidung
# autostart-dienst, Nachtrag): nur diese Unterordner, nicht
# ganz docs; seit 6b auch docs\anleitung. Fehlt einer (docs\zielbild legt erst die Architektur-Rolle an),
# wird er übersprungen, nicht angelegt: Als Administrator angelegt gehörte er
# den Administratoren statt dem Inhaber.
$leseOrdnerWennDa = @('docs\zielbild', 'docs\anleitung')
# Früher (7a1, erste Fassung) bekam web\build eine eigene Regel; sie wird beim Einrichten entfernt.
$alteLeseOrdner = @('web\build')

# ---------------------------------------------------------------------------
# Hilfen
# ---------------------------------------------------------------------------

$script:protokollDatei = $null
$script:puffer = New-Object System.Collections.Generic.List[string]
$script:schritt = 'Vorbereitung'

function Zeile([string]$text) {
    Write-Host $text
    $z = '{0:yyyy-MM-dd HH:mm:ss} {1}' -f (Get-Date), $text
    if ($script:protokollDatei) {
        [IO.File]::AppendAllText($script:protokollDatei, $z + "`r`n", [Text.UTF8Encoding]::new($false))
    } else {
        $script:puffer.Add($z)
    }
}

function Protokoll-Ab([string]$datei) {
    if ($Pruefen -or $script:protokollDatei) { return }
    $script:protokollDatei = $datei
    if ($script:puffer.Count -gt 0) {
        [IO.File]::AppendAllText($datei, (($script:puffer -join "`r`n") + "`r`n"), [Text.UTF8Encoding]::new($false))
        $script:puffer.Clear()
    }
}

# Startet ein Programm mit genau dieser Befehlszeile (gleich in PowerShell 5.1 und 7).
function Starte([string]$exe, [string]$argumente) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.Arguments = $argumente
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.WorkingDirectory = $sys32
    try {
        $enc = [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage)
        $psi.StandardOutputEncoding = $enc
        $psi.StandardErrorEncoding = $enc
    } catch { }
    $p = [Diagnostics.Process]::Start($psi)
    $aus = $p.StandardOutput.ReadToEndAsync()
    $fehl = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    [pscustomobject]@{ Code = $p.ExitCode; Text = ($aus.Result + $fehl).Trim() }
}

function ScExe([string]$argumente) { Starte $scPfad $argumente }

function Icacls([string]$argumente) {
    $r = Starte $icaclsPfad $argumente
    if ($r.Code -ne 0) { throw "icacls $argumente meldet: $($r.Text)" }
}

# Prozessrechte, die der Dienst laut sc qprivs behält (sortiert); leer, wenn
# keine Liste gesetzt ist (dann hat er alle Rechte seines Kontos).
function Privilegien([string]$name) {
    $r = ScExe "qprivs $name"
    if ($r.Code -ne 0) { throw "sc qprivs $name meldet: $($r.Text)" }
    @([regex]::Matches($r.Text, 'Se[A-Za-z]+Privilege') | ForEach-Object { $_.Value } | Sort-Object -Unique)
}

# Prozessrechte des Dienstes auf $dienstPrivilegien beschränken und mit sc qprivs prüfen.
function BeschraenkeRechte([string]$name, [string]$vor) {
    $r = ScExe ("privs {0} {1}" -f $name, ($dienstPrivilegien -join '/'))
    if ($r.Code -ne 0) { throw "sc privs $name meldet: $($r.Text)" }
    $ist = Privilegien $name
    $soll = @($dienstPrivilegien | Sort-Object -Unique)
    if (($ist -join '/') -ne ($soll -join '/')) {
        throw "Die Prozessrechte von $name sind nicht wie gesetzt (sc qprivs nennt: $(if ($ist) { $ist -join ', ' } else { 'keine Liste' }); erwartet: $($soll -join ', ')). Ohne diese Grenze hätte der Dienst SeImpersonatePrivilege."
    }
    Zeile "${vor}Prozessrechte von $($name): nur $($soll -join ', ') (sc qprivs bestätigt; kein SeImpersonatePrivilege)"
}

$rp = [IO.FileAttributes]::ReparsePoint

# Erster Reparse-Punkt (Junction, symbolischer Link, Verknüpfung) in $pfad
# selbst oder irgendwo darunter; $null, wenn keiner. Folgt keinem: Ein
# Reparse-Punkt wird gemeldet, nicht betreten. (Get-ChildItem -Recurse folgt
# in Windows PowerShell 5.1 verlinkten Ordnern; darum hier von Hand.)
function Erste-Verknuepfung([string]$pfad) {
    $startEintrag = Get-Item -LiteralPath $pfad -Force
    if (($startEintrag.Attributes -band $rp) -ne 0) { return $startEintrag.FullName }
    if (-not $startEintrag.PSIsContainer) { return $null }
    $stapel = New-Object System.Collections.Generic.Stack[string]
    $stapel.Push($startEintrag.FullName)
    while ($stapel.Count -gt 0) {
        $d = New-Object IO.DirectoryInfo($stapel.Pop())
        foreach ($e in $d.EnumerateFileSystemInfos()) {
            if (($e.Attributes -band $rp) -ne 0) { return $e.FullName }
            if (($e.Attributes -band [IO.FileAttributes]::Directory) -ne 0) { $stapel.Push($e.FullName) }
        }
    }
    return $null
}

function Besitzer([string]$pfad) { (Get-Acl -LiteralPath $pfad).GetOwner([Security.Principal.SecurityIdentifier]).Value }

# Profil des Inhaber-Kontos (-Konto, sonst das Konto, das das Skript startet),
# aus der Registry (ProfileList); $null, wenn es sich nicht auflösen lässt.
function ProfilPfad {
    try {
        $sid = if ($script:Konto) { (New-Object Security.Principal.NTAccount($script:Konto)).Translate([Security.Principal.SecurityIdentifier]).Value } else { [Security.Principal.WindowsIdentity]::GetCurrent().User.Value }
        return [Environment]::ExpandEnvironmentVariables((Get-ItemProperty -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$sid").ProfileImagePath)
    } catch { return $null }
}

# Prüft den Inhalts-Ordner aus -Inhalt und die Ordner aus [inhalt] in
# dienst.toml (Inhalt-Unsicher). $null, wenn alles gut ist; sonst der erste Satz.
function InhaltOrteUnsicher([string]$datei = $tomlPfad) {
    $profil = ProfilPfad
    if ($Inhalt) {
        $u = Inhalt-Unsicher $Inhalt $profil $repo
        if ($u) { return $u }
    }
    foreach ($o in @(InhaltOrdner $datei | Where-Object { $_.AusInhalt })) {
        $u = Inhalt-Unsicher $o.Pfad $profil $repo
        if ($u) { return "[inhalt] in dienst.toml: $u" }
    }
    return $null
}

function Kontoname([string]$sid) {
    try { return (New-Object Security.Principal.SecurityIdentifier($sid)).Translate([Security.Principal.NTAccount]).Value } catch { return $sid }
}

# Unterordner, die das Einrichten unter der Wurzel anlegt.
$unterordner = @('bin', 'daten', 'sicherung', 'protokoll', 'protokoll\bote', 'geheim')

# Darf ein schon vorhandener Ordner unter ProgramData benutzt werden? $null,
# wenn ja; sonst der Satz. Er muss den Administratoren oder SYSTEM gehören
# (seine festen Unterordner auch), und weder er noch etwas darin darf ein
# Reparse-Punkt sein.
function Wurzel-Unsicher([string]$pfad) {
    $nachsatz = 'bitte den Ordner prüfen und entfernen, dann erneut einrichten.'
    $i = Get-Item -LiteralPath $pfad -Force
    if (($i.Attributes -band $rp) -ne 0) { return "$pfad ist selbst eine Verknüpfung; $nachsatz" }
    if (-not $i.PSIsContainer) { return "$pfad ist eine Datei, kein Ordner; $nachsatz" }
    foreach ($o in @($pfad) + @($unterordner | ForEach-Object { Join-Path $pfad $_ })) {
        if (-not (Test-Path -LiteralPath $o)) { continue }
        $b = Besitzer $o
        if ($b -ne $sidAdmins -and $b -ne $sidSystem) { return "$o gehört nicht den Administratoren (Besitzer $(Kontoname $b)); $nachsatz" }
    }
    $v = Erste-Verknuepfung $pfad
    if ($v) { return "$pfad enthält eine Verknüpfung ($v); $nachsatz" }
    return $null
}

# Entfernt alle ausdrücklichen Einträge (erlauben und verweigern) an $pfad,
# deren SID nicht in $erlaubt steht. Gibt die Zahl der entfernten Konten zurück.
function Nur-Diese([string]$pfad, [string[]]$erlaubt) {
    $acl = Get-Acl -LiteralPath $pfad
    $fremd = @($acl.GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]) |
        ForEach-Object { $_.IdentityReference.Value } | Where-Object { $erlaubt -notcontains $_ } | Sort-Object -Unique)
    foreach ($s in $fremd) { Icacls ('"{0}" /remove *{1}' -f $pfad, $s) }
    return $fremd.Count
}

# Merker: Ordner, die dieser Lauf angelegt hat (nur die darf er wieder löschen).
$script:angelegt = @{}

# Darf aus %USERPROFILE%\.var\dashboard kopiert werden? $null, wenn ja (oder
# wenn es nichts zu kopieren gibt); sonst der Satz. Weder .var noch
# .var\dashboard noch etwas darin darf ein Reparse-Punkt sein.
function Var-Unsicher {
    $var = Join-Path $script:profil '.var'
    $alt = Join-Path $var 'dashboard'
    foreach ($p in @($var, $alt)) {
        if (-not (Test-Path -LiteralPath $p)) { return $null }
        if (((Get-Item -LiteralPath $p -Force).Attributes -band $rp) -ne 0) {
            return "$p ist eine Verknüpfung; kopiert wird nur ohne Verknüpfungen. Bitte prüfen und entfernen (oder den Datenspeicher von Hand umziehen), dann erneut einrichten."
        }
    }
    $v = Erste-Verknuepfung $alt
    if ($v) { return "In $alt liegt eine Verknüpfung ($v); kopiert wird nur ohne Verknüpfungen. Bitte prüfen und entfernen, dann erneut einrichten." }
    return $null
}

# Löscht einen Testordner, den dieser Lauf angelegt hat, ohne Verknüpfungen
# zu folgen. Erst bekommen alle außer SYSTEM und Administratoren ihre
# ausdrücklichen Einträge entzogen (dann kann niemand sonst mehr etwas
# hineinlegen), dann wird nach Reparse-Punkten gesucht; nur ohne sie wird
# gelöscht. $null, wenn gelöscht; sonst der Satz.
function Loesche-Testordner([string]$pfad) {
    if (-not $script:angelegt.ContainsKey($pfad.ToLowerInvariant())) {
        return "Testordner $pfad bleibt liegen: Dieser Lauf hat ihn nicht angelegt."
    }
    foreach ($o in @($pfad) + @($unterordner | ForEach-Object { Join-Path $pfad $_ })) {
        if ((Test-Path -LiteralPath $o -PathType Container) -and -not ((Get-Item -LiteralPath $o -Force).Attributes -band $rp)) {
            $null = Nur-Diese $o @($sidSystem, $sidAdmins)
        }
    }
    $v = Erste-Verknuepfung $pfad
    if ($v) { return "Testordner $pfad enthält eine Verknüpfung ($v) und wird nicht gelöscht; bitte den Ordner prüfen und entfernen." }
    Remove-Item -LiteralPath $pfad -Recurse -Force
    return $null
}

# SID des virtuellen Dienstkontos NT SERVICE\<name>. Sie ist aus dem Namen
# berechnet; sc showsid kennt sie also auch vor dem Anlegen und nach dem Löschen.
function DienstSid([string]$name) {
    $r = ScExe "showsid $name"
    if ($r.Text -match '(S-1-5-80(-\d+)+)') { return $Matches[1] }
    throw "Die SID von NT SERVICE\$name ließ sich nicht ermitteln: $($r.Text)"
}

function DienstStatus([string]$name) {
    $s = Get-Service -Name $name -ErrorAction SilentlyContinue
    if ($s) { return [string]$s.Status }
    return $null
}

function WarteAufStatus([string]$name, [string]$ziel, [int]$sekunden) {
    $ende = (Get-Date).AddSeconds($sekunden)
    while ((Get-Date) -lt $ende) {
        if ((DienstStatus $name) -eq $ziel) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return ((DienstStatus $name) -eq $ziel)
}

function StoppeDienst([string]$name) {
    $st = DienstStatus $name
    if (-not $st -or $st -eq 'Stopped') { return }
    $r = ScExe "stop $name"
    if (-not (WarteAufStatus $name 'Stopped' 20)) {
        throw "Der Dienst $name hielt nicht binnen 20 s an ($($r.Text))."
    }
}

function EntferneDienst([string]$name) {
    if (-not (Get-Service -Name $name -ErrorAction SilentlyContinue)) { return }
    StoppeDienst $name
    $r = ScExe "delete $name"
    # 1072: schon zum Löschen vorgemerkt.
    if ($r.Code -ne 0 -and $r.Code -ne 1072) { throw "sc delete $($name) meldet: $($r.Text)" }
}

# Lebenszeichen über 127.0.0.1 (schneller als localhost; der Server hört auf
# beiden). WindowsAnfrage (Anfrage.ps1) meldet sich als das Konto an, das
# dieses Skript ausführt (der Inhaber, auch erhöht unter UAC dieselbe SID): seit
# Etappe 7b verlangt /api/daten das. Das curl von Windows kann das
# nicht mehr (kein NTLM, siehe Anfrage.ps1). Die Adresse für den
# Browser ist http://localhost:<port>.
. (Join-Path $PSScriptRoot 'Anfrage.ps1')
function HttpCode([int]$p, [string]$weg) {
    (WindowsAnfrage -Url "http://127.0.0.1:$p$weg" -Sekunden 3).Code
}

# Antwort als Objekt; $null ohne Antwort oder ohne JSON.
function HttpJson([int]$p, [string]$weg) {
    $r = Starte $curlPfad "-s --max-time 5 http://127.0.0.1:$p$weg"
    if ($r.Code -ne 0 -or -not $r.Text) { return $null }
    try { return ($r.Text | ConvertFrom-Json) } catch { return $null }
}

# Gibt $null zurück, wenn alle Wege 200 liefern, sonst eine Beschreibung.
function WarteAuf200([int]$p, [string[]]$wege, [int]$sekunden) {
    $ende = (Get-Date).AddSeconds($sekunden)
    $codes = @{}
    do {
        $alle = $true
        foreach ($w in $wege) {
            $c = HttpCode $p $w
            $codes[$w] = $c
            if ($c -ne '200') { $alle = $false }
        }
        if ($alle) { return $null }
        Start-Sleep -Seconds 1
    } while ((Get-Date) -lt $ende)
    return (($wege | ForEach-Object { "$_ -> $($codes[$_])" }) -join ', ')
}

function LetzteZeilen([string]$ordner, [int]$n) {
    $f = Get-ChildItem -LiteralPath $ordner -Filter 'dienst-*.log' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime | Select-Object -Last 1
    if (-not $f) { return @() }
    return @(Get-Content -LiteralPath $f.FullName -Tail $n -Encoding UTF8)
}

# Wer lauscht auf dem Port? $null, wenn frei.
function PortBelegung([int]$p, [string]$dienst) {
    $c = @(Get-NetTCPConnection -LocalPort $p -State Listen -ErrorAction SilentlyContinue)
    if ($c.Count -eq 0) { return $null }
    $prozessId = [int]$c[0].OwningProcess
    $pfad = $null
    try { $pfad = (Get-Process -Id $prozessId -ErrorAction Stop).Path } catch { }
    $dienstPid = 0
    $d = Get-CimInstance Win32_Service -Filter "Name='$dienst'" -ErrorAction SilentlyContinue
    if ($d) { $dienstPid = [int]$d.ProcessId }
    $istDienst = ($dienstPid -ne 0 -and $dienstPid -eq $prozessId)
    [pscustomobject]@{
        ProzessId    = $prozessId
        Pfad         = $pfad
        IstDienst    = $istDienst
        IstDashboard = ($istDienst -or ($pfad -and $pfad -like '*\dashboard.exe'))
    }
}

function TomlWert([string]$text, [string]$schluessel) {
    $m = [regex]::Match($text, '(?m)^\s*' + $schluessel + '\s*=\s*(?:"([^"]*)"|(\d+))')
    if (-not $m.Success) { return $null }
    if ($m.Groups[1].Success) { return $m.Groups[1].Value }
    return $m.Groups[2].Value
}

function Gleicher-Pfad([string]$a, [string]$b) {
    if (-not $a -or -not $b) { return $false }
    (($a -replace '/', '\').TrimEnd('\')) -ieq (($b -replace '/', '\').TrimEnd('\'))
}

function Liegt-In([string]$pfad, [string]$ordner) {
    if (-not $pfad -or -not $ordner) { return $false }
    $o = ($ordner -replace '/', '\').TrimEnd('\') + '\'
    (($pfad -replace '/', '\').TrimEnd('\') + '\').StartsWith($o, [StringComparison]::OrdinalIgnoreCase)
}

function Mit-Schraegstrich([string]$pfad) { $pfad -replace '\\', '/' }

# Ein Abschnitt der TOML-Datei ([name] bis zum nächsten [..]); sonst ''.
function TomlAbschnitt([string]$text, [string]$name) {
    $m = [regex]::Match($text, '(?ms)^\s*\[' + [regex]::Escape($name) + '\]\s*$(.*?)(?=^\s*\[|\z)')
    if ($m.Success) { return $m.Groups[1].Value }
    return ''
}

# Der Abschnitt [inhalt] für einen Inhalts-Ordner (Umzug des Inhalts).
function InhaltText([string]$ordner) {
    @(
        '[inhalt]'
        ('projekte = ["{0}"]' -f (Mit-Schraegstrich (Join-Path $ordner 'projects')))
        ('daten = "{0}"' -f (Mit-Schraegstrich (Join-Path $ordner 'data')))
        ('entscheidungen = "{0}"' -f (Mit-Schraegstrich (Join-Path $ordner 'docs\entscheidungen')))
    ) -join "`n"
}

# Die Ordner des Inhalts, die das Dienstkonto lesen muss: aus [inhalt] in
# dienst.toml (jede Quelle, daten; entscheidungen, wenn es sie gibt), sonst
# wie vor dem Umzug projects, data und docs\entscheidungen im Repo.
# Je Ordner: Pfad und ob er da sein muss.
function InhaltOrdner([string]$datei = $tomlPfad) {
    $t = ''
    if ($datei -and (Test-Path -LiteralPath $datei -PathType Leaf)) { $t = [IO.File]::ReadAllText($datei) }
    $abschnitt = TomlAbschnitt $t 'inhalt'
    $m = [regex]::Match($abschnitt, '(?ms)^\s*projekte\s*=\s*\[(.*?)\]')
    if (-not $abschnitt -or -not $m.Success) {
        return @(
            [pscustomobject]@{ Pfad = (Join-Path $repo 'projects'); Pflicht = $true }
            [pscustomobject]@{ Pfad = (Join-Path $repo 'data'); Pflicht = $true }
            [pscustomobject]@{ Pfad = (Join-Path $repo 'docs\entscheidungen'); Pflicht = $false }
        )
    }
    $v = @()
    foreach ($q in [regex]::Matches($m.Groups[1].Value, '"([^"]*)"')) { $v += [pscustomobject]@{ Pfad = ($q.Groups[1].Value -replace '/', '\'); Pflicht = $true; AusInhalt = $true } }
    $d = TomlWert $abschnitt 'daten'
    if ($d) { $v += [pscustomobject]@{ Pfad = ($d -replace '/', '\'); Pflicht = $true; AusInhalt = $true } }
    $e = TomlWert $abschnitt 'entscheidungen'
    if ($e) { $v += [pscustomobject]@{ Pfad = ($e -replace '/', '\'); Pflicht = $false; AusInhalt = $true } }
    return $v
}

# Hängt [inhalt] an dienst.toml an, wenn er fehlt (-Inhalt). Steht schon einer
# da, muss er auf denselben Ordner zeigen. Gibt $null oder einen Satz zurück.
function InhaltEintragen([string]$ordner, [bool]$nurZeigen, [string]$datei = $tomlPfad) {
    if (-not (Test-Path -LiteralPath (Join-Path $ordner 'projects') -PathType Container)) { return "Im Inhalts-Ordner $ordner fehlt projects." }
    if (-not (Test-Path -LiteralPath $datei -PathType Leaf)) { return "$datei fehlt; dann bitte ganz einrichten." }
    $t = [IO.File]::ReadAllText($datei)
    $abschnitt = TomlAbschnitt $t 'inhalt'
    if ($abschnitt) {
        if ($abschnitt -match [regex]::Escape((Mit-Schraegstrich (Join-Path $ordner 'projects')))) {
            Zeile "  ok     dienst.toml nennt den Inhalt schon ([inhalt] -> $ordner)"
            return $null
        }
        return "dienst.toml hat schon einen Abschnitt [inhalt], der nicht auf $ordner zeigt. Bitte die Datei prüfen."
    }
    if ((TomlWert (TomlAbschnitt $t 'pfade') 'projekte')) { return "dienst.toml nennt pfade.projekte; das passt nicht zu [inhalt]. Bitte die Zeile erst entfernen." }
    if ($nurZeigen) {
        Zeile "  fehlt  dienst.toml hat noch keinen Abschnitt [inhalt]; würde angehängt (-> $ordner)"
        return $null
    }
    $neu = $t.TrimEnd() + "`n`n# Umzug des Inhalts, angehängt von Dienst-Einrichten.ps1 -Inhalt am $('{0:yyyy-MM-dd}' -f (Get-Date)).`n" + (InhaltText $ordner) + "`n"
    [IO.File]::WriteAllText($datei, $neu, [Text.UTF8Encoding]::new($false))
    Zeile "  gesetzt dienst.toml: Abschnitt [inhalt] angehängt (-> $ordner)"
    return $null
}

# Ordner zwischen Profil und Repo (ohne das Repo), etwa C:\Users\<konto>\dev und C:\Users\<konto>.
function Vorfahren([string]$pfad, [string]$bis) {
    $liste = @()
    if (-not (Liegt-In $pfad $bis)) { return $liste }
    $p = Split-Path -Parent $pfad
    while ($p -and (Liegt-In $p $bis)) {
        $liste += $p
        if (Gleicher-Pfad $p $bis) { break }
        $p = Split-Path -Parent $p
    }
    return $liste
}

function ExpliziteRegeln([string]$pfad, [string]$sid) {
    $acl = Get-Acl -LiteralPath $pfad
    @($acl.GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]) |
        Where-Object { $_.IdentityReference.Value -eq $sid })
}

# Darf das Dienstkonto durch diesen Ordner hindurch (über Jeder, Authentifizierte
# Benutzer, Benutzer, Dienst oder einen eigenen Eintrag)?
function HatDurchlauf([string]$pfad, [string]$dienstSid) {
    $sids = @('S-1-1-0', 'S-1-5-11', 'S-1-5-32-545', 'S-1-5-6', $dienstSid)
    $acl = Get-Acl -LiteralPath $pfad
    foreach ($r in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
        if ($r.AccessControlType -eq 'Allow' -and ($sids -contains $r.IdentityReference.Value) -and
            (($r.FileSystemRights -band [Security.AccessControl.FileSystemRights]::ExecuteFile) -ne 0)) {
            return $true
        }
    }
    return $false
}

# Hat $sid einen eigenen Erlaubt-Eintrag mit mindestens $rechte, vererbt
# (Ordner und Dateien darunter) oder nur für diesen Ordner?
function HatRegel([string]$pfad, [string]$sid, [Security.AccessControl.FileSystemRights]$rechte, [bool]$vererbt) {
    $ci = [Security.AccessControl.InheritanceFlags]::ContainerInherit
    $oi = [Security.AccessControl.InheritanceFlags]::ObjectInherit
    @(ExpliziteRegeln $pfad $sid | Where-Object {
            $_.AccessControlType -eq 'Allow' -and
            (($_.FileSystemRights -band $rechte) -eq $rechte) -and
            ($(if ($vererbt) { (($_.InheritanceFlags -band $ci) -ne 0) -and (($_.InheritanceFlags -band $oi) -ne 0) }
               else { $_.InheritanceFlags -eq [Security.AccessControl.InheritanceFlags]::None }))
        }).Count -gt 0
}

# Alle Orte im Repo und darüber, an denen das Einrichten dem Dienstkonto Rechte gibt.
function RepoOrte {
    $imRepo = @($leseOrdner) + @($leseOrdnerWennDa) + @($alteLeseOrdner) + @('projects', 'data', 'docs\entscheidungen')
    $inhaltOrte = @(InhaltOrdner | ForEach-Object { $_.Pfad })
    @($repo) + @($imRepo | ForEach-Object { Join-Path $repo $_ }) + $inhaltOrte + @(Vorfahren $repo $script:profil) | Select-Object -Unique
}

# Lese-Recht im Repo für das Dienstkonto: <repo> nur lesen (ohne Vererbung),
# die Ordner aus $leseOrdner und $leseOrdnerWennDa (wenn da) lesen und
# ausführen (vererbt).
function GibRepoRechte([string]$dienstSid, [string]$dienstKonto, [string]$vor) {
    $lesen = [Security.AccessControl.FileSystemRights]::Read
    $lesenAusfuehren = [Security.AccessControl.FileSystemRights]::ReadAndExecute
    $alt = @(ExpliziteRegeln $repo $dienstSid | Where-Object { $_.InheritanceFlags -ne [Security.AccessControl.InheritanceFlags]::None })
    if ($alt.Count -gt 0) {
        Zeile "${vor}Nehme die ältere Regel für $dienstKonto auf dem ganzen Repo $repo zurück (vererbt; bei großen Ordnern wie target dauert das etwas) ..."
        Icacls ('"{0}" /remove:g *{1}' -f $repo, $dienstSid)
    }
    if (HatRegel $repo $dienstSid $lesen $false) {
        Zeile "${vor}$dienstKonto darf den Ordner $repo schon öffnen (lesen, ohne Vererbung)"
    } else {
        Icacls ('"{0}" /grant:r *{1}:(R)' -f $repo, $dienstSid)
        Zeile "${vor}$dienstKonto darf den Ordner $repo öffnen (lesen, ohne Vererbung; target, .git bleiben außen vor)"
    }
    # Eigene Regel der ersten Fassung auf web\build entfernen (danach erbt web\build von web).
    foreach ($o in $alteLeseOrdner) {
        $p = Join-Path $repo $o
        if ((Test-Path -LiteralPath $p) -and (ExpliziteRegeln $p $dienstSid).Count -gt 0) {
            Icacls ('"{0}" /remove:g *{1}' -f $p, $dienstSid)
            Zeile "${vor}Ältere eigene Regel für $dienstKonto auf $p entfernt (erbt jetzt von web)"
        }
    }
    foreach ($o in $leseOrdner) {
        $p = Join-Path $repo $o
        if (-not (Test-Path -LiteralPath $p -PathType Container)) { throw "Der Ordner $p fehlt; der Dienst braucht ihn." }
        if (HatRegel $p $dienstSid $lesenAusfuehren $true) {
            Zeile "${vor}$dienstKonto darf $p schon lesen (vererbt)"
        } else {
            Icacls ('"{0}" /grant:r *{1}:(OI)(CI)(RX)' -f $p, $dienstSid)
            Zeile "${vor}$dienstKonto darf $p lesen (vererbt)"
        }
    }
    # Der Inhalt: aus [inhalt] in dienst.toml, sonst projects, data und
    # docs\entscheidungen im Repo (InhaltOrdner). Pflichtordner müssen da sein.
    $wennDa = @($leseOrdnerWennDa | ForEach-Object { Join-Path $repo $_ })
    foreach ($o in @(InhaltOrdner $script:tomlFuerInhalt)) {
        if ($o.Pflicht) {
            if (-not (Test-Path -LiteralPath $o.Pfad -PathType Container)) { throw "Der Ordner $($o.Pfad) fehlt; der Dienst braucht ihn." }
            if (HatRegel $o.Pfad $dienstSid $lesenAusfuehren $true) {
                Zeile "${vor}$dienstKonto darf $($o.Pfad) schon lesen (vererbt)"
            } else {
                Icacls ('"{0}" /grant:r *{1}:(OI)(CI)(RX)' -f $o.Pfad, $dienstSid)
                Zeile "${vor}$dienstKonto darf $($o.Pfad) lesen (vererbt)"
            }
        } else {
            $wennDa += $o.Pfad
        }
    }
    foreach ($p in $wennDa) {
        if (-not (Test-Path -LiteralPath $p -PathType Container)) {
            Zeile "${vor}$p fehlt noch; übersprungen (nicht angelegt). Sobald es ihn gibt, einmal mit -NurLeserecht nachziehen."
        } elseif (HatRegel $p $dienstSid $lesenAusfuehren $true) {
            Zeile "${vor}$dienstKonto darf $p schon lesen (vererbt)"
        } else {
            Icacls ('"{0}" /grant:r *{1}:(OI)(CI)(RX)' -f $p, $dienstSid)
            Zeile "${vor}$dienstKonto darf $p lesen (vererbt)"
        }
    }
}

function EntferneRepoRechte([string]$sid, [string]$name) {
    foreach ($p in (RepoOrte)) {
        if ((Test-Path -LiteralPath $p) -and (ExpliziteRegeln $p $sid).Count -gt 0) {
            Icacls ('"{0}" /remove:g *{1}' -f $p, $sid)
            Zeile "  Rechte von NT SERVICE\$name an $p entfernt"
        }
    }
}

# Kennung des Grundbaus eines Baus der Oberfläche (wie app::grundbau): in
# _app\version.json die 16 Hex-Zeichen vor `-<Bauzeit>`; $null ohne sie.
function Grundbau([string]$ordner) {
    try {
        $v = (Get-Content -LiteralPath (Join-Path $ordner '_app\version.json') -Raw -Encoding UTF8 -ErrorAction Stop | ConvertFrom-Json).version
        if ("$v" -match '^([0-9a-f]{16})-\d+$') { return $Matches[1] }
    } catch { }
    return $null
}

# Die Oberfläche des Dienstes ist eine Kopie in bin\web (app::WebOrt) und gehört
# zu den Programmen. Entschieden wird am Inhalt, nicht am Lauf: Hat bin\web nicht
# denselben Grundbau wie $build (die Kopie neben den Programmen), kommt $build
# dazu, die vorige als bin\web.alt (wie Dienst.ps1 -Neustart); so holt auch ein
# zweiter Lauf nach einem Abbruch zwischen Programmen und Oberfläche sie nach.
# Ein Bau ohne Kennung wird nie eingespielt (er sperrte jeden Bau des Boten).
# Das Skript läuft als Administrator und bin ist für den Inhaber änderbar:
# kopiert und gelöscht wird nur ohne Verknüpfungen. Gibt den Satz für das
# Protokoll zurück, wirft sonst.
function OberflaecheEinspielen([string]$bin, [string]$build) {
    $web = Join-Path $bin 'web'
    $neu = Join-Path $bin 'web.neu'
    $alt = Join-Path $bin 'web.alt'
    $soll = Grundbau $build
    if (-not $soll) { throw "In $build fehlt die Kennung des Grundbaus (_app\version.json); eingespielt wird nur ein Bau von Dienst.ps1 -Neustart. Bitte erst scripts\Dienst.ps1 -Neustart (beim ersten Einrichten in web\ npm run build)." }
    if ((Test-Path -LiteralPath (Join-Path $web 'index.html') -PathType Leaf) -and (Grundbau $web) -eq $soll) { return "Oberfläche in $web ist schon da (Grundbau $soll)" }
    $oben = Split-Path -Parent $build
    if (((Get-Item -LiteralPath $oben -Force).Attributes -band $rp) -ne 0) { throw "$oben ist eine Verknüpfung; kopiert wird nur ohne Verknüpfungen." }
    foreach ($o in @($build, $neu, $alt, $web)) {
        if (-not (Test-Path -LiteralPath $o)) { continue }
        $v = Erste-Verknuepfung $o
        if ($v) { throw "In $o liegt eine Verknüpfung ($v); kopiert und gelöscht wird nur ohne Verknüpfungen. Bitte prüfen und entfernen, dann erneut einrichten." }
    }
    if (Test-Path -LiteralPath $neu) { Remove-Item -LiteralPath $neu -Recurse -Force }
    Copy-Item -LiteralPath $build -Destination $neu -Recurse
    if (-not (Test-Path -LiteralPath (Join-Path $neu 'index.html') -PathType Leaf)) { throw "In $neu fehlt nach dem Kopieren index.html." }
    $vorige = Test-Path -LiteralPath $web
    if ($vorige) {
        if (Test-Path -LiteralPath $alt) { Remove-Item -LiteralPath $alt -Recurse -Force }
        Move-Item -LiteralPath $web -Destination $alt
    }
    Move-Item -LiteralPath $neu -Destination $web
    return "Oberfläche aus $build nach $web kopiert (Grundbau $soll)$(if ($vorige) { " (vorige als $alt)" })"
}

# Erst die Aufgabe anhalten, dann Wächter (--waechter) und Boten beenden,
# den Wächter zuerst, damit er den Boten nicht neu startet.
function StoppeBoten([string]$boteExe, [string]$ordner, [string]$name) {
    $a = Get-ScheduledTask -TaskPath $ordner -TaskName $name -ErrorAction SilentlyContinue
    if ($a -and $a.State -eq 'Running') { Stop-ScheduledTask -TaskPath $ordner -TaskName $name }
    $gesamt = 0
    $ende = (Get-Date).AddSeconds(5)
    do {
        $laufend = @(Get-CimInstance Win32_Process -Filter "Name='dashboard-bote.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.ExecutablePath -and (Gleicher-Pfad $_.ExecutablePath $boteExe) } |
            Sort-Object @{ Expression = { if ([string]$_.CommandLine -match '(^|\s)--waechter(\s|$)') { 0 } else { 1 } } })
        foreach ($pr in $laufend) { try { Stop-Process -Id $pr.ProcessId -Force -ErrorAction Stop; $gesamt++ } catch { } }
        if ($laufend.Count -eq 0) { break }
        Start-Sleep -Milliseconds 300
    } while ((Get-Date) -lt $ende)
    if ($gesamt -gt 0) { Zeile "  Wächter und Bote beendet ($gesamt Prozesse)" }
}

trap {
    Zeile "FEHLER im Schritt '$($script:schritt)': $($_.Exception.Message)"
    Zeile 'Das Skript hat hier aufgehört und nichts weiter geändert. scripts\Dienst-Entfernen.ps1 (als Administrator) nimmt alles zurück, was bis hier eingerichtet ist.'
    exit 1
}

# ---------------------------------------------------------------------------
# Einen Dienst samt Ordner, Rechten, Programmen und Einstellungen einrichten.
# Mit -Test für den Probedienst: ohne Daten, ohne Boten, ohne Rechte für den Inhaber am Dienst.
# ---------------------------------------------------------------------------
function Richte-Ein {
    param([string]$Name, [string]$Wurzel, [int]$P, [string]$Pipe, [switch]$Test)

    $vor = if ($Test) { '  Probe: ' } else { '' }
    $bin = Join-Path $Wurzel 'bin'
    $daten = Join-Path $Wurzel 'daten'
    $sicherung = Join-Path $Wurzel 'sicherung'
    $protokoll = Join-Path $Wurzel 'protokoll'
    $boteProtokoll = Join-Path $protokoll 'bote'
    # Etappe 7c: Schlüssel der Ausweise (JWT); die Datei legt der Dienst selbst an.
    $geheim = Join-Path $Wurzel 'geheim'
    $toml = Join-Path $Wurzel 'dienst.toml'
    $exe = Join-Path $bin 'dashboard.exe'
    $inhaber = $script:kontoSid

    # Dienst anlegen (Start von Hand, bis alles fertig ist), damit es das Konto gibt.
    $script:schritt = "Dienst $Name anlegen"
    $binArg = 'binPath= "\"{0}\" --dienst --einstellungen \"{1}\" --dienstname {2}"' -f $exe, $toml, $Name
    $anzeige = if ($Test) { "$Name (Probe)" } else { "$Name (Inhaber)" }
    $dienstKonto = "NT SERVICE\$Name"
    $w = Get-CimInstance Win32_Service -Filter "Name='$Name'" -ErrorAction SilentlyContinue
    if ($w) {
        $a = "config $Name $binArg DisplayName= `"$anzeige`""
        if ($w.StartName -ne $dienstKonto) { $a += " obj= `"$dienstKonto`"" }
        $r = ScExe $a
        if ($r.Code -ne 0) { throw "sc config meldet: $($r.Text)" }
        Zeile "${vor}Dienst $Name war schon da; Programmpfad und Konto erneuert"
    } else {
        $r = ScExe "create $Name $binArg obj= `"$dienstKonto`" start= demand DisplayName= `"$anzeige`""
        if ($r.Code -ne 0) { throw "sc create meldet: $($r.Text)" }
        Zeile "${vor}Dienst $Name angelegt (Konto $dienstKonto, noch ohne Start beim Hochfahren)"
    }
    $r = ScExe "description $Name `"Lokales Dashboard, im Browser http://localhost:$P. Einrichten und Entfernen mit scripts\Dienst-Einrichten.ps1 und scripts\Dienst-Entfernen.ps1.`""
    if ($r.Code -ne 0) { throw "sc description meldet: $($r.Text)" }

    # Prozessrechte beschränken (vor dem ersten Start; gilt ab dem nächsten Start).
    $script:schritt = "Prozessrechte von $Name beschränken (sc privs)"
    BeschraenkeRechte $Name $vor
    $dienstSid = DienstSid $Name

    # Ordner und Rechte. Unter ProgramData darf jedes Konto Ordner anlegen;
    # ein vorhandener Ordner kann also vorbesetzt sein.
    $script:schritt = "Ordner $Wurzel prüfen und anlegen"
    # Vererbung aus; SYSTEM und Administratoren voll; der Inhaber darf den Ordner selbst sehen.
    $wurzelRechte = '"{0}" /inheritance:r /grant:r *{1}:(OI)(CI)(F) *{2}:(OI)(CI)(F) *{3}:(RX)' -f $Wurzel, $sidSystem, $sidAdmins, $inhaber
    if (Test-Path -LiteralPath $Wurzel) {
        $u = Wurzel-Unsicher $Wurzel
        if ($u) { throw $u }
        Zeile "${vor}Ordner $Wurzel ist schon da: gehört den Administratoren (oder SYSTEM), keine Verknüpfung darin"
    } else {
        New-Item -ItemType Directory -Path $Wurzel | Out-Null
        $script:angelegt[$Wurzel.ToLowerInvariant()] = $true
        # Sofort sperren: Besitzer Administratoren, Vererbung von ProgramData
        # aus (dort dürfen alle Benutzer Ordner anlegen). Danach muss er noch
        # leer sein; sonst hat jemand in der Lücke etwas hineingelegt.
        Icacls ('"{0}" /setowner *{1}' -f $Wurzel, $sidAdmins)
        Icacls $wurzelRechte
        if (@(Get-ChildItem -LiteralPath $Wurzel -Force).Count -gt 0) {
            throw "In $Wurzel lag gleich nach dem Anlegen schon etwas; bitte den Ordner prüfen und entfernen, dann erneut einrichten."
        }
        Zeile "${vor}Ordner $Wurzel angelegt und gleich gesperrt (Besitzer Administratoren, Vererbung aus)"
    }
    $neu = 0
    foreach ($o in @($bin, $daten, $sicherung, $protokoll, $boteProtokoll, $geheim)) {
        if (-not (Test-Path -LiteralPath $o)) { New-Item -ItemType Directory -Path $o | Out-Null; $neu++ }
    }
    Zeile ("${vor}Unterordner bin, daten, sicherung, protokoll, protokoll\bote, geheim: {0}" -f $(if ($neu) { "$neu angelegt" } else { 'schon da' }))

    $script:schritt = "Rechte an $Wurzel setzen"
    Icacls ('"{0}" /setowner *{1}' -f $Wurzel, $sidAdmins)
    Icacls $wurzelRechte
    Icacls ('"{0}" /grant:r *{1}:(OI)(CI)(RX) *{2}:(OI)(CI)(M)' -f $bin, $dienstSid, $inhaber)
    Icacls ('"{0}" /grant:r *{1}:(OI)(CI)(M)' -f $daten, $dienstSid)
    Icacls ('"{0}" /remove:g *{1}' -f $daten, $inhaber)
    Icacls ('"{0}" /grant:r *{1}:(OI)(CI)(M) *{2}:(OI)(CI)(R)' -f $sicherung, $dienstSid, $inhaber)
    Icacls ('"{0}" /grant:r *{1}:(OI)(CI)(M) *{2}:(OI)(CI)(R)' -f $protokoll, $dienstSid, $inhaber)
    # protokoll\bote: Der Bote läuft als Inhaber und schreibt hier sein Protokoll,
    # also ändern für den Inhaber. Das Dienstkonto räumt alte Dateien weg (löschen
    # braucht ändern). Vererbung aus und genau diese vier Einträge: Sonst stünden
    # die von protokoll geerbten neben den eigenen (Inhaber ändern und lesen,
    # Dienstkonto zweimal ändern). /inheritance:r plus /grant:r ist idempotent.
    Icacls ('"{0}" /inheritance:r /grant:r *{1}:(OI)(CI)(F) *{2}:(OI)(CI)(F) *{3}:(OI)(CI)(M) *{4}:(OI)(CI)(M)' -f $boteProtokoll, $sidSystem, $sidAdmins, $inhaber, $dienstSid)
    # geheim (Etappe 7c): Vererbung aus, nur SYSTEM, Administratoren und das
    # Dienstkonto (ändern, damit es beim ersten Start jwt-schluessel anlegen kann);
    # der Inhaber und alle anderen Konten nichts.
    Icacls ('"{0}" /inheritance:r /grant:r *{1}:(OI)(CI)(F) *{2}:(OI)(CI)(F) *{3}:(OI)(CI)(M)' -f $geheim, $sidSystem, $sidAdmins, $dienstSid)
    # Fremde ausdrückliche Einträge weg: Es bleiben nur die oben gesetzten.
    $weg = 0
    $weg += Nur-Diese $Wurzel @($sidSystem, $sidAdmins, $inhaber)
    $weg += Nur-Diese $bin @($dienstSid, $inhaber)
    $weg += Nur-Diese $daten @($dienstSid)
    $weg += Nur-Diese $sicherung @($dienstSid, $inhaber)
    $weg += Nur-Diese $protokoll @($dienstSid, $inhaber)
    $weg += Nur-Diese $boteProtokoll @($sidSystem, $sidAdmins, $inhaber, $dienstSid)
    $weg += Nur-Diese $geheim @($sidSystem, $sidAdmins, $dienstSid)
    Zeile "${vor}Rechte gesetzt: Besitzer Administratoren, Vererbung aus, SYSTEM und Administratoren voll; $dienstKonto ändern auf daten, sicherung, protokoll, lesen auf bin; dein Konto ändern auf bin und protokoll\bote, lesen auf sicherung und protokoll, nichts auf daten; protokoll\bote ohne Vererbung, nur SYSTEM, Administratoren, dein Konto, $dienstKonto; geheim ohne Vererbung, nur SYSTEM, Administratoren, $dienstKonto (ändern)$(if ($weg) { "; $weg fremde Einträge entfernt" })"
    if (-not $Test) { Protokoll-Ab (Join-Path $protokoll ('einrichten-{0:yyyy-MM-dd}.log' -f (Get-Date))) }

    # Programme.
    $script:schritt = "Programme nach $bin kopieren"
    foreach ($n in $programme) {
        $q = Join-Path $quelle $n
        $z = Join-Path $bin $n
        if ((Test-Path -LiteralPath $z) -and ((Get-FileHash -LiteralPath $z).Hash -eq (Get-FileHash -LiteralPath $q).Hash)) {
            Zeile "${vor}$n ist schon aktuell"
            continue
        }
        if (Test-Path -LiteralPath $z) {
            Move-Item -LiteralPath $z -Destination (Join-Path $bin ($n -replace '\.exe$', '.alt.exe')) -Force
            Copy-Item -LiteralPath $q -Destination $z
            Zeile "${vor}$n erneuert (vorige Fassung als $($n -replace '\.exe$', '.alt.exe'))"
        } else {
            Copy-Item -LiteralPath $q -Destination $z
            Zeile "${vor}$n kopiert"
        }
    }

    $script:schritt = "Oberfläche nach $bin\web kopieren"
    Zeile ("${vor}" + (OberflaecheEinspielen $bin $quelleWeb))

    # Datenspeicher (nur der echte Dienst).
    if (-not $Test) {
        $script:schritt = 'Datenspeicher kopieren'
        $alt = Join-Path $script:profil '.var\dashboard'
        $altDatei = Join-Path $alt 'dashboard.redb'
        $zielDatei = Join-Path $daten 'dashboard.redb'
        # Nur ohne Verknüpfungen kopieren: sonst holte das Skript (als
        # Administrator) eine fremde Datei, auf die ein Link zeigt, in den
        # Ordner des Dienstes.
        $u = Var-Unsicher
        if ($u) { throw $u }
        if (Test-Path -LiteralPath $zielDatei) {
            Zeile "Datenspeicher unter $daten ist schon da; er wird nicht überschrieben"
        } elseif (Test-Path -LiteralPath $altDatei) {
            Copy-Item -LiteralPath $altDatei -Destination $zielDatei
            Zeile "Datenspeicher kopiert: $altDatei -> $zielDatei (das Original bleibt liegen)"
        } else {
            Zeile "Kein bisheriger Datenspeicher unter $alt; der Dienst legt einen neuen an"
        }
        $altSich = Join-Path $alt 'sicherung'
        if (Test-Path -LiteralPath $altSich) {
            $kopiert = 0
            foreach ($f in Get-ChildItem -LiteralPath $altSich -File) {
                $z = Join-Path $sicherung $f.Name
                if (-not (Test-Path -LiteralPath $z)) { Copy-Item -LiteralPath $f.FullName -Destination $z; $kopiert++ }
            }
            Zeile "Sicherungen: $kopiert kopiert nach $sicherung (die Originale bleiben liegen)"
        }
    }

    # Einstellungen.
    $script:schritt = "Einstellungen $toml schreiben"
    if (Test-Path -LiteralPath $toml) {
        $t = [IO.File]::ReadAllText($toml)
        if ((TomlWert $t 'inhaber_sid') -ne $inhaber) { throw "$toml nennt ein anderes Konto (inhaber_sid). Bitte die Datei prüfen oder löschen und neu einrichten." }
        if (-not (Gleicher-Pfad (TomlWert $t 'repo') $repo)) { throw "$toml nennt ein anderes Repo ($(TomlWert $t 'repo')) als $repo. Bitte die Datei prüfen oder löschen und neu einrichten." }
        $tDienst = TomlWert $t 'dienst_sid'
        if ($tDienst -and $tDienst -ne $dienstSid) { throw "$toml nennt ein anderes Dienstkonto (dienst_sid $tDienst statt $dienstSid). Bitte die Datei prüfen oder löschen und neu einrichten." }
        if (-not $tDienst) {
            # Ältere Datei ohne dienst_sid: direkt unter inhaber_sid nachtragen, sonst nichts ändern.
            $zeile = 'dienst_sid = "{0}"' -f $dienstSid
            $neuT = ([regex]'(?m)^(\s*inhaber_sid\s*=[^\r\n]*)').Replace($t, ('$1' + "`n" + $zeile), 1)
            if ($neuT -eq $t) { throw "In $toml fehlt dienst_sid, und inhaber_sid ist nicht zu finden. Bitte die Datei prüfen oder löschen und neu einrichten." }
            [IO.File]::WriteAllText($toml, $neuT, [Text.UTF8Encoding]::new($false))
            Zeile "${vor}dienst.toml: dienst_sid nachgetragen ($dienstSid)"
        }
        Zeile "${vor}dienst.toml ist schon da und passt (Port $(TomlWert $t 'port'))"
    } else {
        $tomlText = @(
            "# Einstellungen des Dienstes $Name. Angelegt von scripts\Dienst-Einrichten.ps1 am $('{0:yyyy-MM-dd}' -f (Get-Date))."
            'schema = 1'
            ('repo = "{0}"' -f (Mit-Schraegstrich $repo))
            "port = $P"
            '[pfade]'
            ('daten = "{0}"' -f (Mit-Schraegstrich (Join-Path $daten 'dashboard.redb')))
            ('sicherung = "{0}"' -f (Mit-Schraegstrich $sicherung))
            ('protokoll = "{0}"' -f (Mit-Schraegstrich $protokoll))
            '[konten]'
            ('inhaber_sid = "{0}"' -f $inhaber)
            ('dienst_sid = "{0}"' -f $dienstSid)
            '[bote]'
            ('pipe = "{0}"' -f $Pipe)
            ('programm = "{0}"' -f (Mit-Schraegstrich (Join-Path $bin 'dashboard-bote.exe')))
        ) -join "`n"
        [IO.File]::WriteAllText($toml, $tomlText + "`n", [Text.UTF8Encoding]::new($false))
        Zeile "${vor}dienst.toml geschrieben (Port $P)"
    }
    # Umzug des Inhalts: mit -Inhalt den Abschnitt [inhalt] anhängen (wenn er fehlt).
    if ($Inhalt) {
        $s = InhaltOrteUnsicher $toml
        if ($s) { throw $s }
        $s = InhaltEintragen $Inhalt $false $toml
        if ($s) { throw $s }
    }
    Icacls ('"{0}" /grant:r *{1}:(RX) *{2}:(R)' -f $toml, $dienstSid, $inhaber)

    # Lese-Recht: web, docs\zielbild und docs\anleitung im Repo, der Inhalt aus [inhalt]
    # (sonst projects, data, docs\entscheidungen im Repo); das Repo selbst
    # ohne Vererbung.
    $script:schritt = 'Lese-Recht im Repo und im Inhalt'
    $script:tomlFuerInhalt = $toml
    GibRepoRechte $dienstSid $dienstKonto $vor
    foreach ($o in (Vorfahren $repo $script:profil)) {
        if (HatDurchlauf $o $dienstSid) {
            Zeile "${vor}Durchgang durch $($o): erlaubt"
        } elseif ($Durchlaufrecht) {
            Icacls ('"{0}" /grant *{1}:(X)' -f $o, $dienstSid)
            Zeile "${vor}Durchgang durch $($o): $dienstKonto bekommt nur das Durchlaufrecht (X), ohne Inhalt, ohne Vererbung"
        } else {
            Zeile "${vor}Durchgang durch $($o): kein eigener Eintrag; Windows lässt Dienste trotzdem hindurch (Auslassen der durchsuchenden Überprüfung)"
        }
    }

    if (-not $Test) {
        # Dienst einstellen.
        $script:schritt = 'Dienst einstellen (Neustart nach Fehlern, Rechte)'
        $r = ScExe "failure $Name reset= 3600 actions= restart/5000/restart/30000/restart/120000"
        if ($r.Code -ne 0) { throw "sc failure meldet: $($r.Text)" }
        $r = ScExe "failureflag $Name 1"
        if ($r.Code -ne 0) { throw "sc failureflag meldet: $($r.Text)" }
        Zeile 'Neustart nach Fehlern: nach 5 s, 30 s, 2 min; Zähler nach 1 Stunde zurück; auch ein Ende mit Fehlercode zählt'
        $sd = ((ScExe "sdshow $Name").Text -split "`r?`n" | Where-Object { $_ -match '^D:' } | Select-Object -First 1)
        if (-not $sd) { throw 'sc sdshow lieferte keine Rechte.' }
        # Nur Abfragen und Start/Stopp für den Inhaber:
        #   CC  SERVICE_QUERY_CONFIG          Einstellungen lesen
        #   LC  SERVICE_QUERY_STATUS          Zustand abfragen
        #   SW  SERVICE_ENUMERATE_DEPENDENTS  abhängige Dienste auflisten
        #   RP  SERVICE_START                 starten
        #   WP  SERVICE_STOP                  stoppen
        #   LO  SERVICE_INTERROGATE           nachfragen (Zustand neu melden lassen)
        #   RC  READ_CONTROL                  Rechte lesen
        # Nicht: DT (anhalten/fortsetzen), CR (eigene Steuerbefehle), DC, SD, WD, WO.
        $ace = "(A;;CCLCSWRPWPLORC;;;$inhaber)"
        if ($sd.Contains($ace)) {
            Zeile 'Dein Konto darf den Dienst schon abfragen, starten und stoppen'
        } else {
            # Einen älteren Eintrag für den Inhaber (etwa mit DT und CR) ersetzen.
            $sd = [regex]::Replace($sd, '\(A;;[A-Z]+;;;' + [regex]::Escape($inhaber) + '\)', '')
            $i = $sd.IndexOf('S:(')
            $neuSd = if ($i -ge 0) { $sd.Insert($i, $ace) } else { $sd + $ace }
            $r = ScExe "sdset $Name $neuSd"
            if ($r.Code -ne 0) { throw "sc sdset meldet: $($r.Text)" }
            Zeile 'Dein Konto darf den Dienst jetzt abfragen, starten und stoppen (nicht umkonfigurieren, nicht löschen)'
        }

        # Bote.
        $script:schritt = 'Aufgabe für den Boten anlegen'
        $boteExe = Join-Path $bin 'dashboard-bote.exe'
        $vorhanden = Get-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName -ErrorAction SilentlyContinue
        # Die Aufgabe startet den Wächter; er startet den Boten und nach einem
        # Fehler neu. RestartCount greift, falls der Wächter selbst endet.
        $aktion = New-ScheduledTaskAction -Execute $boteExe -Argument "--waechter --einstellungen `"$toml`"" -WorkingDirectory $bin
        $ausloeser = New-ScheduledTaskTrigger -AtLogOn -User $script:kontoName
        $wer = New-ScheduledTaskPrincipal -UserId $script:kontoName -LogonType Interactive -RunLevel Limited
        $einst = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 `
            -RestartInterval (New-TimeSpan -Minutes 1) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
            -Hidden -MultipleInstances IgnoreNew
        Register-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName -Action $aktion -Trigger $ausloeser `
            -Principal $wer -Settings $einst -Force `
            -Description 'Bote des Dashboard-Dienstes: erledigt, was nur unter dem eigenen Konto geht (Tresor, Benutzer-Daten).' | Out-Null
        Zeile ("Aufgabe {0}{1} {2} (startet bei jeder Anmeldung den Wächter des Boten, ohne Fenster)" -f $aufgabenOrdner, $aufgabenName, $(if ($vorhanden) { 'erneuert' } else { 'angelegt' }))

        # LIESMICH.
        $script:schritt = 'LIESMICH.txt ablegen'
        $lm = Join-Path $Wurzel 'LIESMICH.txt'
        Copy-Item -LiteralPath $liesmichQuelle -Destination $lm -Force
        Icacls ('"{0}" /grant:r *{1}:(R)' -f $lm, $inhaber)
        Zeile "LIESMICH.txt liegt in $Wurzel"
    }
}

# ---------------------------------------------------------------------------
# Probe mit dem Testdienst
# ---------------------------------------------------------------------------
function Probe {
    $tName = "$Dienstname-Test"
    $tWurzel = Join-Path $programData $tName
    $fehler = $null
    $zeilen = @()
    $script:schritt = 'Probe mit dem Testdienst'
    Zeile "Probe: Testdienst $tName auf Port $TestPort in $tWurzel"
    # Ein Testordner, den nicht dieser Lauf angelegt hat, wird weder benutzt
    # noch gelöscht (jedes Konto darf ihn unter ProgramData anlegen).
    if (Test-Path -LiteralPath $tWurzel) {
        throw "Der Testordner $tWurzel liegt schon da (Rest eines früheren Laufs oder von jemand anderem angelegt). scripts\Dienst-Entfernen.ps1 räumt ihn weg, wenn er den Administratoren gehört und keine Verknüpfung enthält; sonst bitte den Ordner prüfen und entfernen, dann erneut einrichten."
    }
    try {
        EntferneDienst $tName
        Richte-Ein -Name $tName -Wurzel $tWurzel -P $TestPort -Pipe "$($Dienstname.ToLower())-test-bote" -Test
        $script:schritt = 'Probe mit dem Testdienst'
        $r = ScExe "start $tName"
        if ($r.Code -ne 0) {
            $fehler = "Der Testdienst startete nicht ($($r.Text))."
        } else {
            $f = WarteAuf200 $TestPort @('/healthz', '/') 30
            if ($f) {
                $fehler = "Der Testdienst antwortete nicht binnen 30 s mit 200 ($f)."
                if ((HttpCode $TestPort '/healthz') -eq '200') {
                    $fehler += ' /healthz geht, / nicht: vermutlich darf das Dienstkonto web oder projects nicht lesen. Einmal mit -Durchlaufrecht versuchen.'
                }
            }
        }
        if (-not $fehler) {
            Zeile '  Probe: /healthz und / antworten mit 200'
            # Läuft alles ohne SeImpersonatePrivilege? Anmeldung (SSPI) und
            # Bote (Pipe) sind die Teile, die Rechte des Prozesses brauchen könnten.
            $wer = HttpJson $TestPort '/api/anmeldung/wer'
            if (-not $wer -or $wer.verfuegbar -ne $true) {
                $fehler = "/api/anmeldung/wer meldet nicht verfuegbar: true ($(if ($wer) { $wer | ConvertTo-Json -Compress } else { 'keine Antwort' })); die Windows-Anmeldung geht mit den beschränkten Prozessrechten offenbar nicht."
            } else {
                Zeile '  Probe: /api/anmeldung/wer meldet verfuegbar: true (ohne SeImpersonatePrivilege)'
            }
        }
        if (-not $fehler) {
            # Ganze Aushandlung mit dem Konto dieses Fensters: 200 (Inhaber) oder
            # 403 (anderes Konto) heißen beide, dass sie gelang; 500 nicht.
            $c = (WindowsAnfrage -Url "http://localhost:$TestPort/api/anmeldung/probe" -Sekunden 5).Code
            if ($c -eq '500') {
                $fehler = '/api/anmeldung/probe antwortet mit 500: Die Windows-Anmeldung scheitert mit den beschränkten Prozessrechten.'
            } elseif ($c -eq '200' -or $c -eq '403') {
                Zeile "  Probe: Windows-Anmeldung (Negotiate) ausgehandelt, Antwort $c"
            } else {
                Zeile "  Probe: Windows-Anmeldung nicht ganz ausgehandelt (Antwort $c; Netzwerk), zählt nicht als Fehler"
            }
        }
        if (-not $fehler) {
            $betrieb = HttpJson $TestPort '/api/betrieb'
            if (-not $betrieb -or -not $betrieb.bote -or $betrieb.bote -eq 'nicht vorgesehen') {
                $fehler = "/api/betrieb sieht keinen Boten vor (bote: $(if ($betrieb) { $betrieb.bote } else { 'keine Antwort' })); die Einstellungen oder die Pipe-Anbindung stimmen nicht."
            } else {
                Zeile "  Probe: /api/betrieb nennt den Boten '$($betrieb.bote)' (frei nur vorgesehen oder nicht vorgesehen; beim Testdienst läuft ohnehin kein Bote)"
            }
        }
        if ($fehler) {
            $w = Get-CimInstance Win32_Service -Filter "Name='$tName'" -ErrorAction SilentlyContinue
            if ($w) { $fehler += " Zustand: $($w.State), Exit-Code $($w.ExitCode), Dienst-Code $($w.ServiceSpecificExitCode)." }
            $zeilen = LetzteZeilen (Join-Path $tWurzel 'protokoll') 10
        }
    } finally {
        try { EntferneDienst $tName; Zeile "  Probe: Testdienst $tName gestoppt und gelöscht" }
        catch { Zeile "  WARNUNG: Testdienst $tName ließ sich nicht löschen: $($_.Exception.Message)" }
        try { EntferneRepoRechte (DienstSid $tName) $tName }
        catch { Zeile "  WARNUNG: Rechte von NT SERVICE\$tName am Repo ließen sich nicht entfernen: $($_.Exception.Message)" }
        try {
            if (Test-Path -LiteralPath $tWurzel) {
                $u = Loesche-Testordner $tWurzel
                if ($u) { Zeile "  WARNUNG: $u" } else { Zeile "  Probe: Testordner $tWurzel entfernt" }
            }
        } catch { Zeile "  WARNUNG: Testordner $tWurzel ließ sich nicht löschen: $($_.Exception.Message)" }
    }
    if ($fehler) {
        foreach ($z in $zeilen) { Zeile "    | $z" }
        Zeile "FEHLER: Die Probe mit dem Testdienst ist gescheitert. $fehler"
        Zeile 'Am echten Dienst wurde nichts angelegt; der Testdienst ist wieder entfernt. Das bisherige Dashboard läuft wie vorher.'
        exit 1
    }
}

# Haupt-Repo auf main? Gibt $null zurück, sonst den Grund als Satzteil.
function GitWarnung {
    $gitPfad = Join-Path $repo '.git'
    if (Test-Path -LiteralPath $gitPfad -PathType Leaf) {
        return "$repo ist ein Git-Worktree (.git ist eine Datei)"
    }
    if (Test-Path -LiteralPath (Join-Path $gitPfad 'HEAD') -PathType Leaf) {
        $kopf = ([IO.File]::ReadAllText((Join-Path $gitPfad 'HEAD'))).Trim()
        $branch = if ($kopf -match '^ref:\s*refs/heads/(.+)$') { $Matches[1] } else { "(kein Branch, $kopf)" }
        if ($branch -ne 'main') { return "$repo steht auf Branch $branch, nicht auf main" }
        return $null
    }
    return "$repo ist kein Git-Repo (kein .git)"
}

# ---------------------------------------------------------------------------
# Nur das Lese-Recht nachziehen (-NurLeserecht): für einen schon
# eingerichteten Dienst nur Schritt 7, sonst nichts. Kein Testdienst, kein
# Anhalten, nichts unter ProgramData außer der Zeile im Protokoll.
# ---------------------------------------------------------------------------
function Nur-Leserecht {
    $script:schritt = 'Lese-Recht im Repo (nur dieser Schritt)'
    Zeile ("Dienst-Einrichten.ps1 -NurLeserecht {0}, Repo {1}" -f $(if ($Pruefen) { '(nur prüfen, ändert nichts)' } else { '(Lese-Recht setzen)' }), $repo)
    $w = GitWarnung
    if ($w) {
        Zeile "  WARNUNG $w. Bitte aus dem Haupt-Repo (%USERPROFILE%\dev\dashboard) auf main laufen lassen."
        if (-not $Pruefen -and -not $Trotzdem) { Zeile 'Abbruch (oder mit -Trotzdem). Es wurde nichts geändert.'; exit 1 }
    }
    if (-not (Get-CimInstance Win32_Service -Filter "Name='$Dienstname'" -ErrorAction SilentlyContinue)) {
        Zeile "Der Dienst $Dienstname ist nicht eingerichtet. Dann bitte ganz einrichten (ohne -NurLeserecht). Es wurde nichts geändert."
        exit 1
    }
    $sid = DienstSid $Dienstname
    $dienstKonto = "NT SERVICE\$Dienstname"
    $rx = [Security.AccessControl.FileSystemRights]::ReadAndExecute
    $fehlt = 0
    # Der Inhalts-Ordner (-Inhalt und [inhalt]): absolut, im Profil, ohne ..,
    # nicht Laufwerk, Profil oder Repo, keine Verknüpfung auf dem Weg.
    $u = InhaltOrteUnsicher
    if ($u) { Zeile "Abbruch: $u Es wurde nichts geändert."; exit 1 }
    # Umzug des Inhalts: mit -Inhalt die Ordner dort (projects, data;
    # docs\entscheidungen, wenn da), sonst aus dienst.toml ([inhalt] oder Repo).
    if ($Inhalt) {
        $s = InhaltEintragen $Inhalt $true
        if ($s) { Zeile "Abbruch: $s Es wurde nichts geändert."; exit 1 }
        $inhaltListe = @(
            [pscustomobject]@{ Pfad = (Join-Path $Inhalt 'projects'); Pflicht = $true }
            [pscustomobject]@{ Pfad = (Join-Path $Inhalt 'data'); Pflicht = $true }
            [pscustomobject]@{ Pfad = (Join-Path $Inhalt 'docs\entscheidungen'); Pflicht = $false }
        )
    } else {
        $inhaltListe = @(InhaltOrdner)
    }
    $liste = @($leseOrdner | ForEach-Object { [pscustomobject]@{ Pfad = (Join-Path $repo $_); Pflicht = $true } }) +
        @($leseOrdnerWennDa | ForEach-Object { [pscustomobject]@{ Pfad = (Join-Path $repo $_); Pflicht = $false } }) + $inhaltListe
    foreach ($o in $liste) {
        $p = $o.Pfad
        if (-not (Test-Path -LiteralPath $p -PathType Container)) {
            if (-not $o.Pflicht) { Zeile "  info   $p fehlt noch; wird übersprungen" }
            else { Zeile "  FEHLT  $p (Ordner fehlt; der Dienst braucht ihn)"; $fehlt++ }
        } elseif (HatRegel $p $sid $rx $true) {
            Zeile "  ok     $dienstKonto darf $p lesen (vererbt)"
        } else {
            Zeile "  fehlt  $dienstKonto darf $p nicht lesen; wird gesetzt"
        }
    }
    if ($fehlt -gt 0) { Zeile 'Es fehlt ein Ordner. Es wurde nichts geändert.'; exit 1 }
    if ($Pruefen) {
        Zeile 'Nur geprüft, nichts geändert. Zum Setzen ohne -Pruefen als Administrator starten.'
        exit 0
    }
    $prot = Join-Path $wurzel 'protokoll'
    if (Test-Path -LiteralPath $prot -PathType Container) { Protokoll-Ab (Join-Path $prot ('einrichten-{0:yyyy-MM-dd}.log' -f (Get-Date))) }
    if ($Inhalt) {
        $s = InhaltEintragen $Inhalt $false
        if ($s) { Zeile "Abbruch: $s"; exit 1 }
    }
    GibRepoRechte $sid $dienstKonto ''
    Zeile ''
    Zeile "Fertig. $dienstKonto darf jetzt die Ordner im Repo und im Inhalt lesen, die das Dashboard braucht; ein Neustart ist nicht nötig$(if ($Inhalt) { ', außer für den neuen Abschnitt [inhalt]: scripts\Dienst.ps1 -Neustart (ohne Administrator)' })."
    Zeile '  Läuft es?     scripts\Dienst.ps1 -Status'
    exit 0
}

# ---------------------------------------------------------------------------
# Nur den Ordner geheim (-NurGeheim): für einen vor 7c eingerichteten Dienst.
# Legt nur C:ProgramData<Dienstname>geheim an (oder setzt seine Rechte)
# und nichts sonst; keinen Schlüssel, kein Anhalten, keine Programme.
# ---------------------------------------------------------------------------
function Nur-Geheim {
    $script:schritt = 'Ordner geheim (nur dieser Schritt)'
    $geheim = Join-Path $wurzel 'geheim'
    Zeile ("Dienst-Einrichten.ps1 -NurGeheim {0}, Ordner {1}" -f $(if ($Pruefen) { '(nur prüfen, ändert nichts)' } else { '(Ordner anlegen und Rechte setzen)' }), $geheim)
    $w = GitWarnung
    if ($w) {
        Zeile "  WARNUNG $w. Bitte aus dem Haupt-Repo (%USERPROFILE%\dev\dashboard) auf main laufen lassen."
        if (-not $Pruefen -and -not $Trotzdem) { Zeile 'Abbruch (oder mit -Trotzdem). Es wurde nichts geändert.'; exit 1 }
    }
    if (-not (Get-CimInstance Win32_Service -Filter "Name='$Dienstname'" -ErrorAction SilentlyContinue)) {
        Zeile "Der Dienst $Dienstname ist nicht eingerichtet. Dann bitte ganz einrichten (ohne -NurGeheim); das legt geheim mit an. Es wurde nichts geändert."
        exit 1
    }
    if (-not (Test-Path -LiteralPath $wurzel -PathType Container)) {
        Zeile "$wurzel fehlt. Dann bitte ganz einrichten (ohne -NurGeheim). Es wurde nichts geändert."
        exit 1
    }
    # Besitzer und Verknüpfungen unter ProgramData kann nur ein Administrator
    # lesen; beim Prüfen ohne Administrator nur zeigen, was sichtbar ist.
    if ($istAdmin) {
        $u = Wurzel-Unsicher $wurzel
        if ($u) { Zeile "Abbruch: $u Es wurde nichts geändert."; exit 1 }
    }
    $sid = DienstSid $Dienstname
    $dienstKonto = "NT SERVICE\$Dienstname"
    $m = [Security.AccessControl.FileSystemRights]::Modify
    $da = Test-Path -LiteralPath $geheim -PathType Container
    if (-not $da) {
        Zeile "  fehlt  $geheim; wird angelegt"
    } elseif (-not $istAdmin) {
        Zeile "  info   $geheim ist da; die Rechte darin sieht nur ein Administrator"
    } elseif (HatRegel $geheim $sid $m $true) {
        Zeile "  ok     $geheim ist da, $dienstKonto darf darin anlegen"
    } else {
        Zeile "  fehlt  $geheim ist da, aber $dienstKonto darf darin nichts anlegen; Rechte werden gesetzt"
    }
    if ($Pruefen) {
        Zeile 'Nur geprüft, nichts geändert. Zum Setzen ohne -Pruefen als Administrator starten.'
        exit 0
    }
    $prot = Join-Path $wurzel 'protokoll'
    if (Test-Path -LiteralPath $prot -PathType Container) { Protokoll-Ab (Join-Path $prot ('einrichten-{0:yyyy-MM-dd}.log' -f (Get-Date))) }
    if (-not $da) { New-Item -ItemType Directory -Path $geheim | Out-Null }
    # Wie beim Einrichten (Etappe 7c): Vererbung aus, nur SYSTEM, Administratoren
    # und das Dienstkonto (ändern, damit es jwt-schluessel anlegen kann).
    Icacls ('"{0}" /inheritance:r /grant:r *{1}:(OI)(CI)(F) *{2}:(OI)(CI)(F) *{3}:(OI)(CI)(M)' -f $geheim, $sidSystem, $sidAdmins, $sid)
    $weg = Nur-Diese $geheim @($sidSystem, $sidAdmins, $sid)
    Zeile "  gesetzt $geheim ohne Vererbung, nur SYSTEM, Administratoren, $dienstKonto (ändern)$(if ($weg) { "; $weg fremde Einträge entfernt" })"
    Zeile ''
    Zeile 'Fertig. Den Schlüssel legt der Dienst beim nächsten Start selbst an.'
    Zeile '  Jetzt neu starten (ohne Administrator):  scripts\Dienst.ps1 -Neustart'
    Zeile '  Danach prüfen:                           scripts\Dienst.ps1 -Status'
    exit 0
}

# ---------------------------------------------------------------------------
# Los
# ---------------------------------------------------------------------------

$ich = [Security.Principal.WindowsIdentity]::GetCurrent()
$istAdmin = (New-Object Security.Principal.WindowsPrincipal($ich)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $Pruefen -and -not $istAdmin) {
    Write-Host "Dienst-Einrichten.ps1 braucht Administrator-Rechte. Bitte ohne Profil starten: Windows-Taste+R, eintippen"
    Write-Host "  $sys32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -NoExit -ExecutionPolicy Bypass -File `"$PSCommandPath`""
    Write-Host 'und mit Strg+Umschalt+Enter als Administrator starten (siehe LIESMICH-Dienst.txt). Nur prüfen geht ohne: -Pruefen'
    exit 5
}

# Erhöhter Lauf härten: Module nur aus Systemordnern, die zwei nötigen mit
# vollem Pfad, Arbeitsordner System32 (für dieses Skript und alles, was es startet).
if ($istAdmin) {
    $env:PSModulePath = (@(
            (Join-Path $PSHOME 'Modules'),
            (Join-Path $programFiles 'WindowsPowerShell\Modules'),
            (Join-Path $sys32 'WindowsPowerShell\v1.0\Modules')
        ) | Select-Object -Unique) -join ';'
    foreach ($modul in @('ScheduledTasks', 'NetTCPIP')) {
        Import-Module -Name (Join-Path $sys32 "WindowsPowerShell\v1.0\Modules\$modul\$modul.psd1") -ErrorAction Stop
    }
    Set-Location -LiteralPath $sys32
    [Environment]::CurrentDirectory = $sys32
}

if ($NurLeserecht) { Nur-Leserecht }
if ($NurGeheim) { Nur-Geheim }

Zeile ("Dienst-Einrichten.ps1 {0}, Repo {1}" -f $(if ($Pruefen) { '(nur prüfen, ändert nichts)' } else { '(einrichten)' }), $repo)
Zeile 'Prüfen:'
$probleme = New-Object System.Collections.Generic.List[string]
function Pruefe([string]$text, [bool]$gut, [string]$wennNicht) {
    if ($gut) { Zeile "  ok     $text" }
    else { Zeile "  FEHLT  $($text): $wennNicht"; $probleme.Add($text) }
}

foreach ($n in $programme) {
    Pruefe "Programm $n in $quelle" (Test-Path -LiteralPath (Join-Path $quelle $n)) 'bitte erst scripts\Dienst.ps1 -Neustart (beim ersten Einrichten im Repo cargo build --release --locked mit CARGO_TARGET_DIR=%LOCALAPPDATA%\Dashboard\cache\dienst)'
}
Pruefe "Oberfläche $quelleWeb (index.html, Kennung des Grundbaus)" ((Test-Path -LiteralPath (Join-Path $quelleWeb 'index.html') -PathType Leaf) -and (Grundbau $quelleWeb)) 'bitte erst scripts\Dienst.ps1 -Neustart (beim ersten Einrichten in web\ npm run build)'
foreach ($o in $leseOrdner) {
    Pruefe "Ordner $o im Repo (der Dienst bekommt Lese-Recht darauf)" (Test-Path -LiteralPath (Join-Path $repo $o) -PathType Container) 'Ordner fehlt'
}
$uInhalt = InhaltOrteUnsicher
Pruefe 'Inhalts-Ordner (-Inhalt, [inhalt]): absolut, im Profil, ohne .., nicht Laufwerk, Profil oder Repo, keine Verknüpfung auf dem Weg' (-not $uInhalt) $uInhalt
foreach ($o in @(InhaltOrdner)) {
    if ($o.Pflicht) { Pruefe "Ordner $($o.Pfad) (Inhalt; der Dienst bekommt Lese-Recht darauf)" (Test-Path -LiteralPath $o.Pfad -PathType Container) 'Ordner fehlt' }
    elseif (Test-Path -LiteralPath $o.Pfad -PathType Container) { Pruefe "Ordner $($o.Pfad) (Inhalt; der Dienst bekommt Lese-Recht darauf)" $true '' }
    else { Zeile "  info   Ordner $($o.Pfad) fehlt noch; wird übersprungen (nicht angelegt), später mit -NurLeserecht nachziehen" }
}
foreach ($o in $leseOrdnerWennDa) {
    if (Test-Path -LiteralPath (Join-Path $repo $o) -PathType Container) { Pruefe "Ordner $o im Repo (der Dienst bekommt Lese-Recht darauf)" $true '' }
    else { Zeile "  info   Ordner $o fehlt noch; wird übersprungen (nicht angelegt), später mit -NurLeserecht nachziehen" }
}
Pruefe 'scripts\LIESMICH-Dienst.txt' (Test-Path -LiteralPath $liesmichQuelle) 'Datei fehlt im Repo'
Pruefe 'Get-NetTCPConnection (Port-Prüfung)' ([bool](Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue)) 'das Modul NetTCPIP fehlt'

# Profil geladen? Ein Profil liegt im Benutzerordner (ändert jedes Programm des
# Kontos) und liefe im erhöhten Lauf als Administrator.
$profilDateien = @($PROFILE.CurrentUserAllHosts, $PROFILE.CurrentUserCurrentHost) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
$ohneProfil = [Environment]::CommandLine -match '(?i)(^|\s)-(nop|noprofile)(\s|$)'
if ($istAdmin -and -not $ohneProfil -and $profilDateien) {
    Zeile "  WARNUNG Diese Sitzung lief wohl mit Profil ($($profilDateien -join ', ')). Sicherer: neu starten mit powershell.exe -NoProfile (siehe LIESMICH-Dienst.txt)."
}

# Konto des Inhabers.
$script:kontoSid = $null; $script:kontoName = $null; $script:profil = $null
try {
    if ($Konto) {
        $script:kontoSid = (New-Object Security.Principal.NTAccount($Konto)).Translate([Security.Principal.SecurityIdentifier]).Value
    } else {
        $script:kontoSid = $ich.User.Value
    }
    $script:kontoName = (New-Object Security.Principal.SecurityIdentifier($script:kontoSid)).Translate([Security.Principal.NTAccount]).Value
    $script:profil = [Environment]::ExpandEnvironmentVariables(
        (Get-ItemProperty -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$($script:kontoSid)").ProfileImagePath)
    Pruefe "Konto $($script:kontoName) ($($script:kontoSid)), Profil $($script:profil)" $true ''
} catch {
    Pruefe "Konto $(if ($Konto) { $Konto } else { $ich.Name })" $false "ließ sich nicht auflösen ($($_.Exception.Message))"
}
if ($script:kontoSid) {
    Pruefe 'Konto ist nicht das eingebaute Administrator-Konto' ($script:kontoSid -notmatch '-500$') 'bitte mit -Konto <dein Benutzername> starten'
    Pruefe 'Repo liegt im Profil dieses Kontos' (Liegt-In $repo $script:profil) ("$repo liegt nicht unter $($script:profil). Wurde die UAC-Abfrage mit einem eigenen Administrator-Konto bestätigt? Dann mit -Konto <dein Benutzername> starten")
}

# Dienstkonto.
$dienstSid = $null
try { $dienstSid = DienstSid $Dienstname; Pruefe "Dienstkonto NT SERVICE\$Dienstname ($dienstSid)" $true '' }
catch { Pruefe "Dienstkonto NT SERVICE\$Dienstname" $false $_.Exception.Message }

# Dienst schon da?
$dienstDa = Get-CimInstance Win32_Service -Filter "Name='$Dienstname'" -ErrorAction SilentlyContinue
if ($dienstDa) {
    Zeile "  info   Dienst $Dienstname gibt es schon ($($dienstDa.State)); ein Lauf repariert ihn"
    try {
        $ist = Privilegien $Dienstname
        $passt = (($ist -join '/') -eq (@($dienstPrivilegien | Sort-Object -Unique) -join '/'))
        Zeile "  info   Prozessrechte jetzt: $(if ($ist) { $ist -join ', ' } else { 'keine Liste (alle Rechte des Kontos, auch SeImpersonatePrivilege)' })$(if (-not $passt) { '; das Einrichten beschränkt sie' })"
    } catch { Zeile "  info   Prozessrechte nicht lesbar ($($_.Exception.Message)); das Einrichten setzt sie" }
}

# Einstellungen schon da?
$wirkPort = $Port
if (Test-Path -LiteralPath $tomlPfad) {
    try {
        $t = [IO.File]::ReadAllText($tomlPfad)
        $tPort = TomlWert $t 'port'
        if ($tPort -and [int]$tPort -ne $Port) {
            Zeile "  info   $tomlPfad nennt Port $tPort statt $Port; die Datei bleibt und gilt (Port $tPort)"
            $wirkPort = [int]$tPort
        }
        if ($script:kontoSid) { Pruefe 'dienst.toml nennt dasselbe Konto' ((TomlWert $t 'inhaber_sid') -eq $script:kontoSid) "inhaber_sid in $tomlPfad weicht ab; Datei prüfen oder löschen" }
        Pruefe 'dienst.toml nennt dasselbe Repo' (Gleicher-Pfad (TomlWert $t 'repo') $repo) "repo in $tomlPfad ist $(TomlWert $t 'repo'); Datei prüfen oder löschen"
        $tDienst = TomlWert $t 'dienst_sid'
        if (-not $tDienst) { Zeile "  info   $tomlPfad hat noch keine dienst_sid; das Einrichten trägt sie nach" }
        elseif ($dienstSid) { Pruefe 'dienst.toml nennt dasselbe Dienstkonto (dienst_sid)' ($tDienst -eq $dienstSid) "dienst_sid in $tomlPfad ist $tDienst statt $dienstSid; Datei prüfen oder löschen" }
    } catch {
        Zeile "  info   $tomlPfad ist da, aber nicht lesbar ($($_.Exception.Message)); wird beim Einrichten geprüft"
    }
}

# Ports.
$b = PortBelegung $wirkPort $Dienstname
if (-not $b) { Pruefe "Port $wirkPort ist frei" $true '' }
elseif ($b.IstDienst) { Pruefe "Port $($wirkPort): der Dienst $Dienstname lauscht (Prozess $($b.ProzessId))" $true '' }
elseif ($b.IstDashboard) { Pruefe "Port $($wirkPort): das laufende Dashboard lauscht (Prozess $($b.ProzessId)); es wird beendet" $true '' }
else { Pruefe "Port $wirkPort" $false "ein anderes Programm lauscht dort (Prozess $($b.ProzessId) $($b.Pfad)). Bitte beenden oder -Port wählen" }
if (-not $OhneTest) {
    $bt = PortBelegung $TestPort "$Dienstname-Test"
    Pruefe "Port $TestPort für den Testdienst ist frei" (-not $bt) ("Prozess $(if ($bt) { $bt.ProzessId }) lauscht dort (wohl ein Testserver). Bitte beenden oder -TestPort <frei> angeben")
}

# Bisheriger Datenspeicher.
if ($script:profil) {
    $altDatei = Join-Path $script:profil '.var\dashboard\dashboard.redb'
    if (Test-Path -LiteralPath $altDatei) { Zeile "  info   bisheriger Datenspeicher: $altDatei ($([math]::Round((Get-Item -LiteralPath $altDatei).Length / 1MB, 1)) MB)" }
    else { Zeile "  info   kein bisheriger Datenspeicher unter $altDatei; der Dienst legt einen neuen an" }
    try {
        $u = Var-Unsicher
        Pruefe "Keine Verknüpfung in $(Join-Path $script:profil '.var\dashboard') (Quelle der Kopie)" (-not $u) $u
    } catch { Zeile "  info   $(Join-Path $script:profil '.var\dashboard') nicht ganz lesbar ($($_.Exception.Message)); das Einrichten prüft es" }
}

# Ordner unter ProgramData (jedes Konto darf dort Ordner anlegen).
if (Test-Path -LiteralPath $wurzel) {
    try {
        $u = Wurzel-Unsicher $wurzel
        Pruefe "$wurzel gehört den Administratoren und enthält keine Verknüpfung" (-not $u) $u
    } catch {
        Zeile "  info   $wurzel ist da, aber ohne Administrator nicht ganz lesbar ($($_.Exception.Message)); das Einrichten prüft Besitzer und Verknüpfungen"
    }
} else {
    Zeile "  info   $wurzel fehlt noch; das Einrichten legt ihn an und sperrt ihn sofort (Besitzer Administratoren, Vererbung aus)"
}
if (-not $OhneTest) {
    $tWurzelP = Join-Path $programData "$Dienstname-Test"
    Pruefe "Kein alter Testordner $tWurzelP" (-not (Test-Path -LiteralPath $tWurzelP)) 'er liegt noch da; scripts\Dienst-Entfernen.ps1 räumt ihn weg (wenn er den Administratoren gehört und keine Verknüpfung enthält), sonst bitte prüfen und entfernen'
}

# Ältere Regel auf dem ganzen Repo?
if ($dienstSid) {
    try {
        if (@(ExpliziteRegeln $repo $dienstSid | Where-Object { $_.InheritanceFlags -ne [Security.AccessControl.InheritanceFlags]::None }).Count -gt 0) {
            Zeile "  info   NT SERVICE\$Dienstname hat noch eine ältere, vererbte Regel auf dem ganzen Repo; das Einrichten ersetzt sie"
        }
        foreach ($o in $alteLeseOrdner) {
            $p = Join-Path $repo $o
            if ((Test-Path -LiteralPath $p) -and (ExpliziteRegeln $p $dienstSid).Count -gt 0) {
                Zeile "  info   NT SERVICE\$Dienstname hat noch eine eigene Regel auf $p; das Einrichten entfernt sie (web\build erbt dann von web)"
            }
        }
        # Lese-Recht auf die Ordner, die der Dienst liest (seit 4d auch docs\entscheidungen, docs\zielbild; seit 6b docs\anleitung).
        $rx = [Security.AccessControl.FileSystemRights]::ReadAndExecute
        foreach ($o in @($leseOrdner) + @($leseOrdnerWennDa)) {
            $p = Join-Path $repo $o
            if (-not (Test-Path -LiteralPath $p -PathType Container)) { continue }
            if (HatRegel $p $dienstSid $rx $true) { Zeile "  info   NT SERVICE\$Dienstname darf $o schon lesen (vererbt)" }
            else { Zeile "  info   NT SERVICE\$Dienstname darf $o noch nicht lesen; das Einrichten setzt es (oder nur dies: -NurLeserecht)" }
        }
    } catch { Zeile "  info   Rechte an $repo nicht lesbar ($($_.Exception.Message))" }
}

# Durchgang durchs Profil.
if ($dienstSid -and $script:profil) {
    foreach ($o in (Vorfahren $repo $script:profil)) {
        try {
            if (HatDurchlauf $o $dienstSid) { Zeile "  info   Durchgang durch $($o): erlaubt" }
            elseif ($Durchlaufrecht) { Zeile "  info   Durchgang durch $($o): kein Eintrag; -Durchlaufrecht gibt nur (X)" }
            else { Zeile "  info   Durchgang durch $($o): kein Eintrag für Jeder, Benutzer oder das Dienstkonto; Windows lässt Dienste trotzdem hindurch, die Probe zeigt es" }
        } catch { Zeile "  info   Durchgang durch $($o): Rechte nicht lesbar ($($_.Exception.Message))" }
    }
}

# Protokollordner des Boten.
$boteOrdner = Join-Path $wurzel 'protokoll\bote'
# Soll: Vererbung aus, genau vier eigene Einträge (je auf Ordner und Dateien).
$boteSoll = "Vererbung aus; nur SYSTEM voll, Administratoren voll, $($script:kontoName) ändern, NT SERVICE\$Dienstname ändern"
if (-not (Test-Path -LiteralPath $boteOrdner)) {
    Zeile "  info   $boteOrdner fehlt noch; das Einrichten legt ihn an ($boteSoll), dort schreibt der Bote sein Protokoll"
} elseif ($script:kontoSid) {
    try {
        $m = [Security.AccessControl.FileSystemRights]::Modify
        $voll = [Security.AccessControl.FileSystemRights]::FullControl
        $soll = New-Object System.Collections.ArrayList
        [void]$soll.Add(@($sidSystem, $voll)); [void]$soll.Add(@($sidAdmins, $voll)); [void]$soll.Add(@($script:kontoSid, $m))
        if ($dienstSid) { [void]$soll.Add(@($dienstSid, $m)) }
        $acl = Get-Acl -LiteralPath $boteOrdner
        $regeln = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
        $treffer = @($soll | Where-Object {
                $s = $_
                @($regeln | Where-Object { -not $_.IsInherited -and $_.AccessControlType -eq 'Allow' -and
                        $_.IdentityReference.Value -eq $s[0] -and (($_.FileSystemRights -band $s[1]) -eq $s[1]) }).Count -eq 1
            }).Count
        $passt = $acl.AreAccessRulesProtected -and $regeln.Count -eq $soll.Count -and $treffer -eq $soll.Count
        $ist = @($regeln | ForEach-Object { '{0}{1} {2}' -f $(if ($_.IsInherited) { 'geerbt ' } else { '' }), (Kontoname $_.IdentityReference.Value), $_.FileSystemRights })
        Zeile "  info   $($boteOrdner): Soll $boteSoll; $(if ($passt) { 'ok' } else { "jetzt $(if (-not $acl.AreAccessRulesProtected) { 'Vererbung an, ' })$($regeln.Count) Einträge ($($ist -join '; ')); das Einrichten setzt das Soll" })"
    } catch { Zeile "  info   $($boteOrdner): Rechte nicht lesbar ($($_.Exception.Message)); das Einrichten setzt sie" }
}

# Haupt-Repo auf main? Aus einem Worktree oder einem anderen Branch würde der
# Dienst auf dieses Repo zeigen (dienst.toml, Leserechte) statt aufs Haupt-Repo.
$gitWarnung = GitWarnung
if ($gitWarnung) {
    $hauptRepo = if ($script:profil) { Join-Path $script:profil 'dev\dashboard' } else { '%USERPROFILE%\dev\dashboard' }
    Zeile "  WARNUNG $gitWarnung. Bitte aus $hauptRepo auf main einrichten.$(if ($Trotzdem) { ' (-Trotzdem: es geht trotzdem weiter)' } elseif (-not $Pruefen) { ' Ohne -Trotzdem bricht das Einrichten ab.' })"
}

Zeile ''
Zeile 'Wird gemacht:'
$plan = @(
    if (-not $OhneTest) { "Probe: Testdienst $Dienstname-Test auf Port $TestPort einrichten (auch mit beschränkten Prozessrechten), starten, / und /healthz auf 200 prüfen, /api/anmeldung/wer auf verfuegbar: true, die Windows-Anmeldung auf nicht 500, /api/betrieb auf einen vorgesehenen Boten; wieder ganz entfernen, den Testordner nur, wenn dieser Lauf ihn angelegt hat und keine Verknüpfung darin ist (erst dann weiter)" }
    "Laufendes Dashboard auf Port $wirkPort beenden (nur den Prozess dashboard.exe, der dort lauscht)"
    "Dienst $Dienstname anlegen, Konto NT SERVICE\$Dienstname (ohne Passwort)"
    "Prozessrechte von $Dienstname auf $($dienstPrivilegien -join ', ') beschränken (sc privs, Prüfung mit sc qprivs; kein SeImpersonatePrivilege)"
    "Ordner $($wurzel): ist er schon da, nur wenn er den Administratoren (oder SYSTEM) gehört und keine Verknüpfung enthält, sonst Abbruch; neu angelegt sofort gesperrt. Mit bin, daten, sicherung, protokoll, protokoll\bote, geheim; Besitzer Administratoren; Vererbung aus; fremde ausdrückliche Einträge weg; SYSTEM und Administratoren voll; NT SERVICE\$Dienstname ändern auf daten, sicherung, protokoll (samt protokoll\bote), lesen auf bin und dienst.toml; $($script:kontoName) ändern auf bin und protokoll\bote (dort schreibt der Bote sein Protokoll), lesen auf sicherung, protokoll, dienst.toml, nichts auf daten; protokoll\bote ohne Vererbung mit genau SYSTEM, Administratoren, $($script:kontoName), NT SERVICE\$Dienstname; geheim (Schlüssel der Passkey-Ausweise, 7c) ohne Vererbung mit genau SYSTEM, Administratoren und NT SERVICE\$Dienstname (ändern)"
    "Programme aus $quelle nach $wurzel\bin (vorige Fassung als *.alt.exe); Oberfläche aus $quelleWeb nach $wurzel\bin\web, wenn der Grundbau ein anderer ist (vorige als bin\web.alt)"
    "Datenspeicher und Sicherungen aus $(if ($script:profil) { Join-Path $script:profil '.var\dashboard' } else { '%USERPROFILE%\.var\dashboard' }) kopieren, nur wenn unter daten noch keiner liegt und dort keine Verknüpfung ist (Original bleibt)"
    "$tomlPfad schreiben (nur wenn sie fehlt), Port $wirkPort, mit inhaber_sid und dienst_sid$(if ($dienstSid) { " ($dienstSid)" })"
    "Lese-Recht für NT SERVICE\$Dienstname auf $((@($leseOrdner) + @($leseOrdnerWennDa) | ForEach-Object { "$repo\$_" }) -join ', ') (vererbt; docs nur diese Unterordner, fehlende werden übersprungen; web ganz, damit jeder Neubau der Oberfläche es erbt) und auf den Ordner $repo selbst (ohne Vererbung); eine ältere eigene Regel auf web\build wird entfernt, falls vorhanden; target, .git und sonst nichts im Profil$(if ($Durchlaufrecht) { '; dazu Durchlaufrecht (X) auf den Ordnern darüber' })"
    "Dienst: Neustart nach Fehlern (5 s, 30 s, 2 min); $($script:kontoName) darf abfragen, starten, stoppen (sdset CCLCSWRPWPLORC)"
    "Aufgabe $aufgabenOrdner$($aufgabenName): bei jeder Anmeldung von $($script:kontoName) startet bin\dashboard-bote.exe --waechter (der Wächter startet den Boten und nach Fehlern neu)"
    "Dienst und Bote starten, / und /api/daten binnen 60 s auf 200 prüfen, dann Start beim Hochfahren"
)
$nr = 0
foreach ($p in $plan) { $nr++; Zeile ("  {0,2}. {1}" -f $nr, $p) }
Zeile ''

if ($probleme.Count -gt 0) {
    Zeile "Es fehlt etwas ($($probleme.Count), oben mit FEHLT). Es wurde nichts geändert."
    exit 1
}
if ($gitWarnung -and -not $Pruefen -and -not $Trotzdem) {
    Zeile "Abbruch: $gitWarnung. Bitte aus $hauptRepo auf main einrichten (oder mit -Trotzdem). Es wurde nichts geändert."
    exit 1
}
if ($Pruefen) {
    Zeile 'Nur geprüft, nichts geändert. Zum Einrichten ohne -Pruefen als Administrator starten.'
    exit 0
}

# 1. Probe.
if ($OhneTest) { Zeile 'Probe mit dem Testdienst übersprungen (-OhneTest)' } else { Probe }

# 2. Laufendes Dashboard beenden.
$script:schritt = 'Laufendes Dashboard beenden'
$b = PortBelegung $wirkPort $Dienstname
if ($b -and -not $b.IstDienst) {
    if (-not $b.IstDashboard) { throw "Auf Port $wirkPort lauscht inzwischen ein anderes Programm (Prozess $($b.ProzessId) $($b.Pfad))." }
    Stop-Process -Id $b.ProzessId -Force
    $ende = (Get-Date).AddSeconds(10)
    while ((PortBelegung $wirkPort $Dienstname) -and (Get-Date) -lt $ende) { Start-Sleep -Milliseconds 500 }
    if (PortBelegung $wirkPort $Dienstname) { throw "Port $wirkPort ist 10 s nach dem Beenden noch belegt." }
    Zeile "Laufendes Dashboard (Prozess $($b.ProzessId)) beendet"
} else {
    Zeile 'Kein Dashboard von Hand gestartet'
}
if ((DienstStatus $Dienstname) -and (DienstStatus $Dienstname) -ne 'Stopped') {
    StoppeDienst $Dienstname
    Zeile "Dienst $Dienstname für das Erneuern angehalten"
}
StoppeBoten (Join-Path $wurzel 'bin\dashboard-bote.exe') $aufgabenOrdner $aufgabenName

# 3. bis 9. Einrichten.
Richte-Ein -Name $Dienstname -Wurzel $wurzel -P $wirkPort -Pipe $pipeName

# 10. Starten und prüfen.
$script:schritt = 'Dienst starten'
$r = ScExe "start $Dienstname"
if ($r.Code -ne 0 -and $r.Code -ne 1056) { throw "sc start meldet: $($r.Text)" }
if (-not (WarteAufStatus $Dienstname 'Running' 30)) { throw "Der Dienst lief nach 30 s nicht (Zustand $(DienstStatus $Dienstname))." }
Zeile "Dienst $Dienstname läuft"

$script:schritt = 'Boten starten'
try {
    Start-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName
    Zeile 'Bote gestartet'
} catch {
    Zeile "Hinweis: Der Bote startete jetzt nicht ($($_.Exception.Message)); er startet bei der nächsten Anmeldung"
}

$script:schritt = 'Antwort des Dienstes prüfen'
$f = WarteAuf200 $wirkPort @('/', '/api/daten') 60
if ($f) {
    foreach ($z in (LetzteZeilen (Join-Path $wurzel 'protokoll') 10)) { Zeile "    | $z" }
    throw "Der Dienst antwortete nicht binnen 60 s mit 200 ($f)."
}
Zeile '/ und /api/daten antworten mit 200'

$script:schritt = 'Start beim Hochfahren einschalten'
$r = ScExe "config $Dienstname start= auto"
if ($r.Code -ne 0) { throw "sc config start= auto meldet: $($r.Text)" }
Zeile 'Der Dienst startet ab jetzt beim Hochfahren von selbst'

Zeile ''
Zeile "Fertig. Das Dashboard läuft als Dienst $Dienstname unter http://localhost:$wirkPort/ und startet beim Hochfahren, auch vor der Anmeldung."
Zeile '  Läuft es?     scripts\Dienst.ps1 -Status'
Zeile '  Neu starten:  scripts\Dienst.ps1 -Neustart      (nach Änderungen, ohne Administrator)'
Zeile '  Entfernen:    scripts\Dienst-Entfernen.ps1      (als Administrator)'
Zeile "Daten, Sicherung und Protokoll liegen in $wurzel (siehe LIESMICH.txt dort)."
exit 0
