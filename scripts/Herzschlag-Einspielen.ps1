#Requires -Version 7.6
# Herzschlag-Einspielen.ps1 - schaltet den Herzschlag (Stop-Hook der Chats) ein oder aus.
#
# Ein (ohne -Aus):
#   1. Trägt in die Einstellungsdatei des Chat-Clients (-Einstellungen, JSON) unter hooks.Stop
#      eine Gruppe mit dem Hook dashboard-herzschlag.exe ein (-Programm, Frist -Sekunden).
#      Vorhandene Hooks und alle anderen Einstellungen bleiben; ein gleicher eigener Eintrag
#      bleibt an seiner Stelle, ein abweichender wird ersetzt, nie verdoppelt.
#   2. Setzt die Benutzer-Umgebungsvariable DASHBOARD_SITZUNG_VARIABLE auf -Variable, den
#      Namen der Umgebungsvariable, in der der Client die Kennung seiner Sitzung mitgibt.
#      scripts\Api.ps1 liest darüber die Kennung (Anleitung, Abschnitt 16).
# Aus (-Aus): entfernt nur die eigenen Einträge (Befehl mit dashboard-herzschlag.exe) und
#   die Variable; leer gewordene Gruppen, hooks.Stop und hooks fallen weg.
#
# Geschrieben wird erst, wenn die Datei gelesen und geprüft ist: Kaputtes JSON oder eine
# unerwartete Form von hooks bricht ohne Schreiben ab (Exit 1). Vor jeder Änderung liegt eine
# Sicherung daneben (<datei>.<zeit>.bak); ist schon alles so, wird nichts geschrieben.
# Geschrieben wird mit 2 Leerzeichen Einrückung; Zeilenenden, BOM und das Zeilenende am
# Schluss folgen der gelesenen Datei.
# Ein Pfad der Einstellungen steht hier bewusst nicht: Er nennt den Client.
#
# -Umgebung ist der Registry-Schlüssel der Benutzer-Umgebung (Standard HKCU:\Environment);
# Tests geben einen eigenen Schlüssel an. Nur beim Standard erfahren neue Prozesse sofort
# davon (die .NET-Methode sendet die Nachricht über die geänderte Umgebung).
#
# Aufruf:
#   pwsh -NoProfile -File scripts\Herzschlag-Einspielen.ps1 -Einstellungen <datei> -Variable <name>
#   pwsh -NoProfile -File scripts\Herzschlag-Einspielen.ps1 -Einstellungen <datei> -Aus
# Exit: 0 gut (auch: war schon so), 1 Fehler (nichts oder nur der erste Teil geändert, siehe Ausgabe).
param(
	[string]$Einstellungen,
	[string]$Variable,
	[string]$Programm = (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)) 'Dashboard\bin\dashboard-herzschlag.exe'),
	[ValidateRange(15, 600)][int]$Sekunden = 30,
	[string]$Umgebung = 'HKCU:\Environment',
	[switch]$Aus
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)

$name = 'DASHBOARD_SITZUNG_VARIABLE'
$eigen = 'dashboard-herzschlag.exe'

function Fehler([string]$satz) {
	[Console]::Error.WriteLine("Herzschlag-Einspielen.ps1: $satz")
	exit 1
}

if (-not $Einstellungen) { Fehler 'Es fehlt -Einstellungen <datei>, die Einstellungsdatei des Chat-Clients (JSON).' }
if (-not $Aus) {
	if ($Variable -notmatch '^[A-Za-z_][A-Za-z0-9_]{0,99}$') { Fehler 'Es fehlt -Variable <name> (Buchstaben, Ziffern, _), der Name der Umgebungsvariable mit der Sitzungskennung des Clients.' }
	# Absolut, denn der Client startet den Hook in seinem eigenen Arbeitsordner.
	$Programm = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Programm)
	# Am Dateinamen erkennt -Aus den eigenen Eintrag; " $ ` würde die Shell des Clients deuten.
	if ((Split-Path -Leaf $Programm) -ne $eigen -or $Programm -match '["$`]') { Fehler "-Programm muss ein Pfad auf $eigen ohne die Zeichen "" $ `` sein: $Programm" }
	if (-not (Test-Path -LiteralPath $Programm -PathType Leaf)) { Fehler "$Programm gibt es nicht (erst den Dienst mit der neuen Fassung einrichten, oder -Programm angeben)." }
}
$datei = [IO.Path]::GetFullPath($Einstellungen)
$ordner = Split-Path -Parent $datei
if (-not (Test-Path -LiteralPath $ordner -PathType Container)) { Fehler "Den Ordner $ordner gibt es nicht." }
if (-not (Test-Path -LiteralPath $Umgebung)) { Fehler "Den Schlüssel $Umgebung gibt es nicht." }

# Lesen und prüfen. Bis zum Schreiben ändert sich nur das Dokument im Speicher.
$da = Test-Path -LiteralPath $datei -PathType Leaf
if ($da -and ((Get-Item -LiteralPath $datei -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { Fehler "$datei ist eine Verknüpfung; bitte die Zieldatei angeben. Nichts geändert." }
$roh = if ($da) { [IO.File]::ReadAllText($datei) } else { '' }
$bom = $da -and (([IO.File]::ReadAllBytes($datei) | Select-Object -First 3) -join ',') -eq '239,187,191'
$nl = if ($roh.Contains("`r`n")) { "`r`n" } else { "`n" }
$schluss = if (-not $da -or $roh -match '\n\s*\z') { $nl } else { '' }
$dopt = [Text.Json.JsonDocumentOptions]::new()
$dopt.AllowDuplicateProperties = $false
try {
	if ($da) { $dok = [System.Text.Json.Nodes.JsonNode]::Parse($roh, $null, $dopt) } else { $dok = [System.Text.Json.Nodes.JsonObject]::new() }
} catch {
	Fehler "$datei ist kein gültiges JSON ($($_.Exception.GetBaseException().Message)). Nichts geändert."
}
if ($dok -isnot [System.Text.Json.Nodes.JsonObject]) { Fehler "$datei enthält kein JSON-Objekt. Nichts geändert." }
$form = "hooks in $datei hat eine unerwartete Form (erwartet: hooks.Stop als Liste von Gruppen mit einer Liste hooks). Nichts geändert."

$opt = [Text.Json.JsonSerializerOptions]::new()
$opt.WriteIndented = $true
$opt.Encoder = [Text.Encodings.Web.JavaScriptEncoder]::UnsafeRelaxedJsonEscaping
$vorher = $dok.ToJsonString($opt)

function Feld($objekt, [string]$schluessel) {
	$wert = $null
	if ($objekt.TryGetPropertyValue($schluessel, [ref]$wert)) { return , $wert }
	return $null
}

function Ist-Eigen($hook) {
	if ($hook -isnot [System.Text.Json.Nodes.JsonObject]) { return $false }
	$befehl = Feld $hook 'command'
	return ($befehl -is [System.Text.Json.Nodes.JsonValue]) -and ("$($befehl.GetValueKind())" -eq 'String') -and $befehl.ToString().Contains($eigen, [StringComparison]::OrdinalIgnoreCase)
}

$hooks = Feld $dok 'hooks'
if ($null -ne $hooks -and $hooks -isnot [System.Text.Json.Nodes.JsonObject]) { Fehler $form }
$stop = $null
if ($null -ne $hooks) { $stop = Feld $hooks 'Stop' }
if ($null -ne $stop -and $stop -isnot [System.Text.Json.Nodes.JsonArray]) { Fehler $form }

# Vorwärtsstriche: Der Client startet den Befehl über eine Shell, in der \ ein Fluchtzeichen sein kann.
$befehl = '"' + ($Programm -replace '\\', '/') + '"'

# Eigene Einträge entfernen; bei Ein bleibt der erste gleiche an seiner Stelle, damit es nie zwei gibt.
$behalten = $null
if ($null -ne $stop) {
	for ($g = 0; $g -lt $stop.Count; $g++) {
		$gruppe = $stop[$g]
		if ($gruppe -isnot [System.Text.Json.Nodes.JsonObject]) { Fehler $form }
		$liste = Feld $gruppe 'hooks'
		if ($liste -isnot [System.Text.Json.Nodes.JsonArray]) { Fehler $form }
		$hatte = $liste.Count
		for ($h = 0; $h -lt $liste.Count) {
			$hook = $liste[$h]
			if (-not (Ist-Eigen $hook)) { $h++ }
			elseif (-not $Aus -and $null -eq $behalten -and $hook['command'].ToString() -ceq $befehl -and "$($hook['timeout'])" -eq "$Sekunden" -and "$($hook['type'])" -eq 'command') { $behalten = $hook; $h++ }
			else { $liste.RemoveAt($h) }
		}
		if ($hatte -gt 0 -and $liste.Count -eq 0) { $stop.RemoveAt($g); $g-- }
	}
}

if ($Aus) {
	if ($null -ne $stop -and $stop.Count -eq 0) { $null = $hooks.Remove('Stop') }
	if ($null -ne $hooks -and $hooks.Count -eq 0) { $null = $dok.Remove('hooks') }
} elseif ($null -eq $behalten) {
	if ($null -eq $hooks) { $hooks = [System.Text.Json.Nodes.JsonObject]::new(); $dok['hooks'] = $hooks }
	if ($null -eq $stop) { $stop = [System.Text.Json.Nodes.JsonArray]::new(); $hooks['Stop'] = $stop }
	$gruppe = [ordered]@{ hooks = @([ordered]@{ type = 'command'; command = $befehl; timeout = $Sekunden }) } | ConvertTo-Json -Depth 5 -Compress
	$stop.Add([System.Text.Json.Nodes.JsonNode]::Parse($gruppe))
}

$nachher = $dok.ToJsonString($opt)
if ($nachher -ceq $vorher -and ($da -or $Aus)) {
	Write-Output "Hook in $($datei): schon $(if ($Aus) { 'ausgetragen' } else { 'eingetragen' }), nichts geschrieben."
} else {
	if ($da) {
		$sicherung = '{0}.{1:yyyyMMdd-HHmmss-fff}.bak' -f $datei, (Get-Date)
		Copy-Item -LiteralPath $datei -Destination $sicherung
		Write-Output "Sicherung: $sicherung"
	}
	$temp = Join-Path $ordner ('.herzschlag-einspielen-{0}.tmp' -f [guid]::NewGuid().ToString('N'))
	try {
		[IO.File]::WriteAllText($temp, $nachher.ReplaceLineEndings($nl) + $schluss, [Text.UTF8Encoding]::new($bom))
		[IO.File]::Move($temp, $datei, $true)
	} catch {
		Fehler "$datei ließ sich nicht schreiben ($($_.Exception.GetBaseException().Message)). Nichts geändert, die Sicherung bleibt."
	} finally {
		if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Force }
	}
	Write-Output "Hook in $($datei): $(if ($Aus) { 'ausgetragen' } else { "eingetragen ($befehl, Frist $Sekunden s)" })."
}

# Die Umgebungsvariable.
$jetzt = (Get-ItemProperty -LiteralPath $Umgebung -Name $name -ErrorAction SilentlyContinue).$name
$soll = if ($Aus) { $null } else { $Variable }
if ("$jetzt" -ceq "$soll") {
	Write-Output "$($name): schon $(if ($Aus) { 'entfernt' } else { "auf $Variable" }), nichts geändert."
} else {
	if ($Aus) { Remove-ItemProperty -LiteralPath $Umgebung -Name $name }
	else { New-ItemProperty -LiteralPath $Umgebung -Name $name -Value $Variable -PropertyType String -Force | Out-Null }
	if ($Umgebung -eq 'HKCU:\Environment') { [Environment]::SetEnvironmentVariable($name, $soll, 'User') }
	Write-Output "$($name): $(if ($Aus) { 'entfernt' } else { "auf $Variable gesetzt" }). Gilt erst nach einem Neustart des Chat-Clients (er gibt seinen Chats die Umgebung mit, die er beim Start hatte)."
}
exit 0
