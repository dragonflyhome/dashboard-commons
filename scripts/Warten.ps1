# Warten.ps1 - wartet auf Vorgänge für einen Chat und endet erst, wenn es welche gibt.
#
# Leere Antworten (Timeout) führen sofort zu erneutem Warten, Verbindungsfehler
# (Server startet neu) nach 5 s, und das ohne Ende. Ausgegeben wird nur die
# JSON-Antwort des Servers. Je Chat läuft höchstens ein Warten.ps1 zugleich.
#
# Anmeldung (ab Etappe 7b): Die Vorgänge sind nur für das Windows-Konto des Inhabers.
# Das Skript fragt über .NET (Anfrage.ps1, HttpClient mit
# UseDefaultCredentials) und schickt so den Nachweis des angemeldeten Kontos
# mit, ohne Passwort. Früher ging das mit curl.exe --negotiate -u :;
# das curl 8.22 von Windows (KB5127936) kann kein NTLM mehr und
# bekommt nur noch 401.
#
# Exit-Codes:
#   0  Es gibt Vorgänge; die JSON-Antwort steht auf stdout.
#   2  Der Server lehnt die Anfrage als unlesbar ab (422, etwa ungültiger Slug).
#   3  403, 404 oder 503 auch nach 3 Versuchen im Abstand von 10 s (Satz auf stderr).
#   4  Ein anderer HTTP-Code auch nach 3 Versuchen (Satz auf stderr).
#   5  Anfrage.ps1 fehlt neben diesem Skript.
#   6  -Server ist nicht 127.0.0.1[:port] oder localhost[:port].
#   7  Anmeldung: 401 auch nach 3 Versuchen (läuft das Skript unter dem Konto des Inhabers?)
#      oder sofort bei 403 „fremdes Konto“ (Satz auf stderr).
#
# Verwendung in einem Projekt-Chat:
#   1. Bei Arbeitsbeginn alles Offene holen und bearbeiten:
#        pwsh -NoProfile -File %USERPROFILE%/dev/dashboard/scripts/Api.ps1 "api/vorgaenge?fuer=<slug>&offen=1"
#   2. Dieses Skript IM HINTERGRUND starten (als Hintergrundprozess, dessen Ende den Chat weckt):
#        pwsh -NoProfile -File scripts/Warten.ps1 -Slug <slug>
#   3. Endet das Skript, liegen Vorgänge vor. Dann offen lesen (wie Schritt 1),
#      für jeden Vorgang übernehmen:
#        pwsh -NoProfile -File %USERPROFILE%/dev/dashboard/scripts/Api.ps1 api/vorgaenge/<id>/uebernommen -Post
#      handeln und danach erledigen:
#        pwsh -NoProfile -File %USERPROFILE%/dev/dashboard/scripts/Api.ps1 api/vorgaenge/<id>/erledigt -Json '{"notiz":"…"}'
#      und sofort wieder mit Schritt 2 warten. Was nicht übernommen ist, liefert
#      das nächste Warten gleich wieder.
#      Ausnahme Bau-Vorgänge (art "bau"): Die erledigt der Dienst selbst, mit dem
#      Ergebnis im Feld "bau". Das Warten liefert sie genau einmal; nicht
#      übernehmen, nicht erledigen, nur das Ergebnis lesen.
#
# Parameter:
#   -Slug     Projekt bzw. Chat, für den gewartet wird (Pflicht).
#   -Timeout  Sekunden, die der Server höchstens wartet (Standard 1500, höchstens 3600).
#   -Server   Adresse des Dashboards (Standard 127.0.0.1:8787; nur 127.0.0.1 oder localhost).
#   -Seit     Nur Ereignisse nach dieser Nummer (Wiederherstellung); ohne Angabe nicht gesetzt.

param(
  [Parameter(Mandatory)] [string] $Slug,
  [int] $Timeout = 1500,
  [string] $Server = '127.0.0.1:8787',
  [int] $Seit = -1
)

# UTF-8 ohne BOM für die Ausgabe (Umlaute).
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)

function Abbruch([int] $code, [string] $satz) {
  [Console]::Error.WriteLine("Warten.ps1: $satz")
  exit $code
}

if ($Server -notmatch '^(127\.0\.0\.1|localhost)(:\d{1,5})?$') {
  Abbruch 6 "-Server muss 127.0.0.1[:port] oder localhost[:port] sein, nicht '$Server'."
}

$anfragePfad = Join-Path $PSScriptRoot 'Anfrage.ps1'
if (-not (Test-Path -LiteralPath $anfragePfad -PathType Leaf)) {
  Abbruch 5 "Anfrage.ps1 fehlt neben Warten.ps1 ('$anfragePfad')."
}
. $anfragePfad

$url = "http://$Server/api/vorgaenge/warten?fuer=$([uri]::EscapeDataString($Slug))&timeout=$Timeout"
if ($Seit -ge 0) { $url += "&seit=$Seit" }

$saetze = @{
  '401' = 'Der Server verlangt die Windows-Anmeldung; läuft Warten.ps1 unter dem Konto des Inhabers?'
  '403' = 'Der Server weist das Warten ab (403). Läuft Warten.ps1 nicht auf diesem PC, oder ist -Server falsch?'
  '404' = 'Diese Adresse kennt der Server nicht (404). Ist das Dashboard zu alt?'
  '503' = 'Der Datenspeicher ist nicht offen (503). Sieh auf http://localhost:8787/daten nach.'
}
# Fehlversuche mit demselben Fehlercode hintereinander.
$versuche = 0
$letzterCode = ''

while ($true) {
  # GET, darum kein Content-Type. Code '000': keine Verbindung oder Zeit abgelaufen.
  $antwort = WindowsAnfrage -Url $url -Sekunden ($Timeout + 100)
  if ($antwort.Code -eq '000') {
    # Verbindungsfehler oder Abbruch: Server startet vielleicht neu. Weiter warten.
    $versuche = 0
    Start-Sleep -Seconds 5
    continue
  }
  $code = $antwort.Code
  $koerper = $antwort.Text

  if ($code -eq '200') {
    if ($koerper -match '"vorgaenge":\[\{') { $koerper; exit 0 }
    # Leer (Timeout, abgelöst oder Server wird beendet): gleich wieder warten.
    $versuche = 0
    Start-Sleep -Seconds 1
    continue
  }
  # 422: die Anfrage selbst ist falsch (etwa ein ungültiger Slug). Nochmal versuchen hilft nicht.
  if ($code -eq '422') { Abbruch 2 "Der Server lehnt die Anfrage ab (422): $koerper" }
  # 403 mit art "fremdes-konto": angemeldet, aber nicht als Inhaber. Nochmal versuchen hilft nicht.
  if ($code -eq '403' -and $koerper -match '"art"\s*:\s*"fremdes-konto"') {
    Abbruch 7 'Dieses Windows-Konto darf hier nicht.'
  }

  if ($code -ne $letzterCode) { $versuche = 0 }
  $letzterCode = $code
  $versuche++
  if ($versuche -ge 3) {
    if ($code -eq '401') { Abbruch 7 "$($saetze['401']) Antwort: $koerper" }
    if ($saetze.ContainsKey($code)) { Abbruch 3 "$($saetze[$code]) Antwort: $koerper" }
    Abbruch 4 "Unerwartete Antwort des Servers ($code). Antwort: $koerper"
  }
  Start-Sleep -Seconds 10
}
