#Requires -Version 7
# Bauordner-Bereinigen.ps1 - räumt einmal am Tag die Rust-Bauordner auf.
#
# Regel: Bauordner einmal täglich löschen; ein Bau gehört in den Anwendungs-Cache. Drei Teile:
#   Cache    die Bauordner je Rolle (jeder direkte Unterordner von %LOCALAPPDATA%\Dashboard\cache mit
#            CACHEDIR.TAG, etwa arbeit-1, arbeit-2, abnahme, dienst) werden jeden Tag ganz geleert, ohne Altersgrenze, aber nur, wenn gerade kein cargo.exe und kein
#            rustc.exe läuft und keine EINSPIELEN.lock von Dienst.ps1 -Neustart darin liegt (älter als ein
#            Tag gilt sie als verwaist); sonst ein Satz, und der nächste Tag versucht es wieder.
#   Scratch  Bauordner im Arbeitsbereich einer Sitzung (erkannt an CACHEDIR.TAG, das cargo in jede
#            Target-Wurzel schreibt, egal wie der Ordner heißt), in die seit -Tage nichts geschrieben
#            wurde und in die kein laufendes rustc baut.
#   B        in Sitzungen, die nicht laufen (kein Prozess mit ihrer Kennung in der Befehlszeile, 2 h nichts
#            geschrieben): node_modules-Kopien (nie Links), .vite, .eslintcache und *.tmp, seit -Tage unverändert.
# Nie: die Ordner in -Tabu (etwa Verläufe und Daten, die erhalten bleiben sollen), %LOCALAPPDATA%\Dashboard
# außer genau dem Cache, Quellen, Notizen und ganze Sitzungsordner, Links, Ordner ohne Sitzungs-Kennung,
# dev\dashboard\target (main), web\builds (räumt der Bote selbst auf).
#
# -Wurzel (Pflicht) ist der Arbeitsordner der Assistenten: darunter je Projekt ein Ordner, darin ein Unterordner
# je Sitzung (eine GUID als Kennung), und nur in dessen Arbeitsbereich, dem Unterordner scratchpad, räumt das
# Skript; -Tabu nennt
# Ordner, die nie angefasst werden, auch wenn -Wurzel sie umfasst (Pflicht bei -Scharf). Beide stehen nur in der
# geplanten Aufgabe, nicht im Code. Eine -Wurzel, die das Benutzerprofil oder einen Anwendungsdaten-Ordner ist oder
# umfasst, lehnt das Skript ab. Ohne -Scharf nur Trockenlauf. Mit -Scharf eine Zeile je Ordner in -Protokoll.
#
# Aufruf:
#   pwsh -NoProfile -File scripts\Bauordner-Bereinigen.ps1 -Wurzel <ordner> -Tabu <ordner>,<ordner>   # Trockenlauf
#   pwsh -NoProfile -File scripts\Bauordner-Bereinigen.ps1 -Wurzel <ordner> -Tabu <ordner> -Scharf    # löscht
#   pwsh -NoProfile -File scripts\Bauordner-Bereinigen.ps1 -Selbsttest  # prüft die Auswahl an einem Beispielbaum
param(
	[string]$Wurzel,
	[string[]]$Tabu = @(),
	[string]$Protokoll = (Join-Path $env:LOCALAPPDATA 'Dashboard\protokoll\bauordner-bereinigen.log'),
	[double]$Tage = 1,
	[switch]$Scharf,
	[switch]$Selbsttest
)
$ErrorActionPreference = 'Stop'
$kennungMuster = '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'

function Voll([string]$p) { [IO.Path]::GetFullPath($p).TrimEnd('\') }

# Nie über -Wurzel löschen, egal welche: die Ordner aus -Tabu und alles des Dashboards.
$tabu = @(@($Tabu | Where-Object { $_ }) + (Join-Path $env:LOCALAPPDATA 'Dashboard')) | ForEach-Object { (Voll $_) + '\' }

# Zu breit: -Wurzel ist das Benutzerprofil, %APPDATA% oder %LOCALAPPDATA% oder ein Ordner darüber.
function Test-ZuBreit([string]$w) {
	$p = (Voll $w) + '\'
	[bool](@($env:USERPROFILE, $env:APPDATA, $env:LOCALAPPDATA) | Where-Object { $_ -and ((Voll $_) + '\').StartsWith($p, [StringComparison]::OrdinalIgnoreCase) })
}

# Ziele laufender rustc (--out-dir <bauordner>\<profil>\deps), voll.
function Get-Belegt {
	Get-CimInstance Win32_Process -Filter "Name='rustc.exe'" | ForEach-Object {
		if ("$($_.CommandLine)" -match '--out-dir\s+(?:"([^"]+)"|(\S+))') { Voll ($Matches[1] ?? $Matches[2]) }
	} | Sort-Object -Unique
}

function Test-Belegt([string]$pfad, [string[]]$belegt) {
	$p = (Voll $pfad) + '\'
	[bool]($belegt | Where-Object { ($_ + '\').StartsWith($p, [StringComparison]::OrdinalIgnoreCase) })
}

function Get-Bestand([string]$pfad) {
	$dateien = @(Get-ChildItem -LiteralPath $pfad -Recurse -Force -File -ErrorAction SilentlyContinue)
	[pscustomobject]@{
		Bytes   = ($dateien | Measure-Object Length -Sum).Sum
		Neueste = if ($dateien) { ($dateien | Measure-Object LastWriteTime -Maximum).Maximum } else { (Get-Item -LiteralPath $pfad -Force).LastWriteTime }
	}
}

# Bauordner unter `ordner`: Ordner mit CACHEDIR.TAG, ohne Links zu folgen, ohne in einen gefundenen hinabzusteigen
# und ohne node_modules (Vite und Vitest legen dort auch eins an; das gehört zur Umgebung, nicht zum Bau).
function Get-Bauordner([string]$ordner) {
	foreach ($d in Get-ChildItem -LiteralPath $ordner -Directory -Force -ErrorAction SilentlyContinue) {
		if (($d.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $d.Name -eq 'node_modules') { continue }
		if (Test-Path -LiteralPath (Join-Path $d.FullName 'CACHEDIR.TAG')) { $d.FullName; continue }
		Get-Bauordner $d.FullName
	}
}

# Kennungen der Sitzungen, die ein laufender Prozess in seiner Befehlszeile trägt (jeder Prozess:
# lieber eine Sitzung zu viel für laufend halten als die Umgebung einer laufenden wegräumen).
function Get-LaufendeSitzungen {
	Get-CimInstance Win32_Process | ForEach-Object {
		[regex]::Matches("$($_.CommandLine)", '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}') | ForEach-Object Value
	} | Sort-Object -Unique
}

# Teil B, nur in Sitzungen, die nicht laufen: Umgebungsreste unter `ordner`, in die seit `alt` nichts geschrieben
# wurde: node_modules-Kopien (Links nie), .vite, .eslintcache und *.tmp. Ohne in sie hinabzusteigen.
function Get-Umgebungsreste([string]$ordner, [datetime]$alt) {
	foreach ($e in Get-ChildItem -LiteralPath $ordner -Force -ErrorAction SilentlyContinue) {
		if ($e.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
		if ($e.PSIsContainer) {
			if (Test-Path -LiteralPath (Join-Path $e.FullName 'CACHEDIR.TAG')) { continue }   # Bauordner: Teil Scratch
			if ($e.Name -in 'node_modules', '.vite') {
				$b = Get-Bestand $e.FullName
				if ($b.Neueste -lt $alt) { [pscustomobject]@{ Pfad = $e.FullName; Bytes = $b.Bytes; Belegt = $false } }
				continue
			}
			Get-Umgebungsreste $e.FullName $alt
		} elseif (($e.Name -eq '.eslintcache' -or $e.Extension -eq '.tmp') -and $e.LastWriteTime -lt $alt) {
			[pscustomobject]@{ Pfad = $e.FullName; Bytes = $e.Length; Belegt = $false }
		}
	}
}

function Get-Kandidaten([string]$ort, [string[]]$belegt, [datetime]$jetzt, [string[]]$laufend = @()) {
	$alt = $jetzt.AddDays(-$Tage)
	$schon = $jetzt.AddHours(-2)
	# Auch auf dem Weg bis zum Arbeitsbereich keinem Link folgen.
	$echt = { -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) }
	foreach ($projekt in Get-ChildItem -LiteralPath $ort -Directory -Force | Where-Object $echt) {
		foreach ($sitzung in Get-ChildItem -LiteralPath $projekt.FullName -Directory -Force | Where-Object $echt | Where-Object Name -Match $kennungMuster) {
			$scratch = Get-Item -LiteralPath (Join-Path $sitzung.FullName 'scratchpad') -Force -ErrorAction SilentlyContinue
			if (-not $scratch -or ($scratch.Attributes -band [IO.FileAttributes]::ReparsePoint)) { continue }
			$scratch = $scratch.FullName
			foreach ($t in Get-Bauordner $scratch) {
				if (Test-Belegt $t $belegt) { [pscustomobject]@{ Pfad = $t; Bytes = 0; Belegt = $true }; continue }
				$bt = Get-Bestand $t
				if ($bt.Neueste -lt $alt) { [pscustomobject]@{ Pfad = $t; Bytes = $bt.Bytes; Belegt = $false } }
			}
			# Teil B: Sitzung läuft nicht (kein Prozess mit ihrer Kennung, in den letzten 2 h nichts geschrieben).
			if ($laufend -contains $sitzung.Name) { continue }
			if ((Get-Bestand $sitzung.FullName).Neueste -gt $schon) { continue }
			Get-Umgebungsreste $scratch $alt
		}
	}
}

function Test-Erlaubt([string]$pfad, [string]$ort) {
	$voll = Voll $pfad
	if ($tabu | Where-Object { ($voll + '\').StartsWith($_, [StringComparison]::OrdinalIgnoreCase) }) { return $false }
	$w = (Voll $ort) + '\'
	# Nur <wurzel>\<projekt>\<sitzung>\scratchpad\...\<bauordner>, nie die Wurzel, ein Projekt- oder Sitzungsordner.
	return $voll.StartsWith($w, [StringComparison]::OrdinalIgnoreCase) -and ($voll.Substring($w.Length).Split('\').Count -ge 4) -and ($voll -match '\\scratchpad\\.+')
}

function Schreib([string]$zeile, [switch]$loeschen) {
	$zeile
	if ($loeschen) {
		New-Item -ItemType Directory -Path (Split-Path $Protokoll) -Force | Out-Null
		Add-Content -LiteralPath $Protokoll -Value ('{0:yyyy-MM-dd HH:mm} {1}' -f (Get-Date), $zeile) -Encoding utf8
	}
}

# Ein Cache-Platz: nur unter `basis`, nur mit CACHEDIR.TAG (sonst könnte ein falscher Pfad einen beliebigen
# Ordner löschen), kein Link, nur ohne laufenden Bau. `basis` ändert nur der Selbsttest.
function Invoke-Cache([string]$cache, [bool]$bautGerade, [switch]$loeschen, [string]$basis = (Join-Path $env:LOCALAPPDATA 'Dashboard\cache')) {
	if (-not $cache -or -not (Test-Path -LiteralPath $cache)) { return }
	if (-not ((Voll $cache) + '\').StartsWith((Voll $basis) + '\', [StringComparison]::OrdinalIgnoreCase) -or (Voll $cache) -ieq (Voll $basis)) { Schreib "Cache liegt nicht unter $basis, übersprungen: $cache" -loeschen:$loeschen; return }
	if (-not (Test-Path -LiteralPath (Join-Path $cache 'CACHEDIR.TAG'))) { Schreib "Cache ohne CACHEDIR.TAG, übersprungen: $cache" -loeschen:$loeschen; return }
	if ((Get-Item -LiteralPath $cache -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { Schreib "Cache ist ein Link, übersprungen: $cache" -loeschen:$loeschen; return }
	if ($bautGerade) { Schreib "Cache nicht geleert, weil gerade cargo oder rustc läuft: $cache" -loeschen:$loeschen; return }
	# Dienst.ps1 -Neustart spielt aus diesem Platz ein (auch zwischen zwei cargo-Läufen); älter als ein Tag: verwaist.
	$sperre = Get-Item -LiteralPath (Join-Path $cache 'EINSPIELEN.lock') -Force -ErrorAction SilentlyContinue
	if ($sperre -and $sperre.LastWriteTime -gt (Get-Date).AddDays(-1)) { Schreib "Cache nicht geleert, weil gerade eingespielt wird (EINSPIELEN.lock): $cache" -loeschen:$loeschen; return }
	if ($sperre) { Schreib "Sperrdatei älter als ein Tag, verwaist, Cache wird geleert: $cache" -loeschen:$loeschen }
	$b = Get-Bestand $cache
	Schreib ('{0,8:N2} GB  Cache {1}' -f ($b.Bytes / 1GB), $cache) -loeschen:$loeschen
	# Get-Bestand dauert; ein Einspielen, das inzwischen begann, nicht wegräumen.
	if ($loeschen -and -not $sperre -and (Test-Path -LiteralPath (Join-Path $cache 'EINSPIELEN.lock'))) { Schreib "Cache nicht geleert, weil inzwischen eingespielt wird (EINSPIELEN.lock): $cache" -loeschen; return }
	if ($loeschen) { Loesch $cache }
}

# Löscht; blieb etwas stehen (gesperrte Datei), steht „teilweise“ im Protokoll.
function Loesch([string]$pfad) {
	Remove-Item -LiteralPath $pfad -Recurse -Force -ErrorAction Continue
	if (Test-Path -LiteralPath $pfad) { Schreib "teilweise gelöscht (gesperrt): $pfad" -loeschen }
}

# Alle Cache-Plätze: jeder direkte Unterordner der Basis; die Prüfungen macht Invoke-Cache je Ordner.
function Invoke-Caches([string]$basis, [bool]$bautGerade, [switch]$loeschen) {
	foreach ($d in Get-ChildItem -LiteralPath $basis -Directory -Force -ErrorAction SilentlyContinue) { Invoke-Cache $d.FullName $bautGerade -loeschen:$loeschen -basis $basis }
}

function Invoke-Bereinigen([string]$ort, [string[]]$belegt, [switch]$loeschen, [string[]]$laufend = @()) {
	$liste = @(Get-Kandidaten $ort $belegt (Get-Date) $laufend)
	foreach ($k in $liste) {
		if ($k.Belegt) { Schreib "belegt, übersprungen: $($k.Pfad)" -loeschen:$loeschen; continue }
		if (-not (Test-Erlaubt $k.Pfad $ort)) { throw "Nicht erlaubt: $($k.Pfad)" }
		Schreib ('{0,8:N2} GB  {1}' -f ($k.Bytes / 1GB), $k.Pfad) -loeschen:$loeschen
		if ($loeschen) { Loesch $k.Pfad }
	}
	$weg = @($liste | Where-Object { -not $_.Belegt })
	'{0} {1} Ordner in Arbeitsbereichen, {2:N2} GB' -f $(if ($loeschen) { 'Gelöscht:' } else { 'Trockenlauf, würde löschen:' }), $weg.Count, (($weg | Measure-Object Bytes -Sum).Sum / 1GB)
}

if ($Selbsttest) {
	$t = Join-Path ([IO.Path]::GetTempPath()) ("bauordner-test-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
	$Protokoll = Join-Path $t 'protokoll.log'
	$s = @{ a = '11111111-1111-1111-1111-111111111111'; b = '22222222-2222-2222-2222-222222222222' }
	function Neu([string]$rel, [double]$tageAlt) {
		$p = Join-Path $t $rel; New-Item -ItemType File -Path $p -Force | Out-Null
		(Get-Item $p).LastWriteTime = (Get-Date).AddDays(-$tageAlt)
	}
	$sa = "p\$($s.a)\scratchpad"; $sb = "p\$($s.b)\scratchpad"
	try {
		Neu "$sa\alt\Cargo.toml" 5; Neu "$sa\alt\src\main.rs" 5; Neu "$sa\alt\target\CACHEDIR.TAG" 5; Neu "$sa\alt\target\debug\x.bin" 5
		Neu "$sa\alt\target\debug\build\y\CACHEDIR.TAG" 5   # innen: nicht zusätzlich
		Neu "$sa\x-target\CACHEDIR.TAG" 5; Neu "$sa\x-target\debug\x.bin" 5   # CARGO_TARGET_DIR ohne Cargo.toml
		Neu "$sa\warm\CACHEDIR.TAG" 5; Neu "$sa\warm\y.bin" 0.1
		Neu "$sa\belegt-target\CACHEDIR.TAG" 5; Neu "$sa\belegt-target\debug\deps\x.bin" 5
		Neu "$sa\ohne\target\x.bin" 5   # kein CACHEDIR.TAG: bleibt
		Neu "$sa\w\web\node_modules\.vite\vitest\CACHEDIR.TAG" 5   # Umgebung, kein Bau: bleibt
		Neu "geteilt\Cargo.toml" 30; Neu "geteilt\target\CACHEDIR.TAG" 30   # Sitzung als Link: nie folgen
		New-Item -ItemType Junction -Path (Join-Path $t 'p\33333333-3333-3333-3333-333333333333') -Target (Join-Path $t 'geteilt') | Out-Null
		Neu "p\$($s.b)\notiz.md" 30; Neu "$sb\alt\target\CACHEDIR.TAG" 30
		Neu "p\nachsicherung\target\CACHEDIR.TAG" 30
		Neu "ziel\target\CACHEDIR.TAG" 30; New-Item -ItemType Directory -Path (Join-Path $t "$sb\link") -Force | Out-Null
		New-Item -ItemType Junction -Path (Join-Path $t "$sb\link\target") -Target (Join-Path $t 'ziel\target') | Out-Null
		# Teil B: Sitzung b läuft nicht, a läuft.
		Neu "$sb\w\web\node_modules\x.js" 30; Neu "$sb\w\.eslintcache" 30; Neu "$sb\w\a.tmp" 30; Neu "$sb\w\web\.vite\y.js" 30
		Neu "geteilt-nm\x.js" 30; New-Item -ItemType Directory -Path (Join-Path $t "$sb\v\web") -Force | Out-Null
		New-Item -ItemType Junction -Path (Join-Path $t "$sb\v\web\node_modules") -Target (Join-Path $t 'geteilt-nm') | Out-Null
		Neu "$sa\w\web\node_modules\z.js" 30; Neu "$sa\w\b.tmp" 30
		$belegt = @(Voll (Join-Path $t "$sa\belegt-target\debug\deps"))
		$rel = @(Get-Kandidaten $t $belegt (Get-Date) @($s.a)) | ForEach-Object { ($(if ($_.Belegt) { 'belegt ' } else { '' })) + $_.Pfad.Substring($t.Length + 1) } | Sort-Object
		$soll = @("$sa\alt\target", "$sa\x-target", "belegt $sa\belegt-target", "$sb\alt\target", "$sb\w\web\node_modules", "$sb\w\.eslintcache", "$sb\w\a.tmp", "$sb\w\web\.vite") | Sort-Object
		if (($rel -join '|') -ne ($soll -join '|')) { throw "Selbsttest rot. Ist: $($rel -join '; ')" }
		if (Test-Erlaubt (Join-Path $t "p\$($s.a)") $t) { throw 'Selbsttest rot: Sitzungsordner wäre erlaubt' }
		$tabu = @((Voll (Join-Path $t 'verlauf')) + '\') + $tabu
		if (Test-Erlaubt (Join-Path $t "verlauf\x\$($s.a)\scratchpad\a\target") (Join-Path $t 'verlauf')) { throw 'Selbsttest rot: Tabu-Ordner wäre erlaubt' }
		if (-not (Test-ZuBreit $env:USERPROFILE) -or -not (Test-ZuBreit (Split-Path $env:USERPROFILE)) -or (Test-ZuBreit $t)) { throw 'Selbsttest rot: Prüfung auf zu breite -Wurzel' }
		if (Test-Erlaubt (Join-Path $env:LOCALAPPDATA "Dashboard\x\$($s.a)\scratchpad\a\target") (Join-Path $env:LOCALAPPDATA 'Dashboard')) { throw 'Selbsttest rot: Dashboard-Ordner wäre über -Wurzel erlaubt' }
		Invoke-Bereinigen $t $belegt -loeschen -laufend @($s.a) | Out-Null
		foreach ($bleibt in "$sa\alt\src\main.rs", "$sa\alt\Cargo.toml", "$sa\warm\y.bin", "$sa\belegt-target\debug\deps\x.bin", "$sa\ohne\target\x.bin", "p\$($s.b)\notiz.md", 'p\nachsicherung\target\CACHEDIR.TAG', 'ziel\target\CACHEDIR.TAG', "$sa\w\web\node_modules\.vite\vitest\CACHEDIR.TAG", 'geteilt\target\CACHEDIR.TAG', "$sa\w\web\node_modules\z.js", "$sa\w\b.tmp", 'geteilt-nm\x.js', "$sb\v\web\node_modules") {
			if (-not (Test-Path (Join-Path $t $bleibt))) { throw "Selbsttest rot: $bleibt gelöscht" }
		}
		foreach ($weg in "$sa\alt\target", "$sa\x-target", "$sb\alt\target", "$sb\w\web\node_modules", "$sb\w\.eslintcache", "$sb\w\a.tmp", "$sb\w\web\.vite") {
			if (Test-Path (Join-Path $t $weg)) { throw "Selbsttest rot: $weg nicht gelöscht" }
		}
		# Cache: nur unter der Basis und mit CACHEDIR.TAG; ohne laufenden Bau geleert, mit nicht; der Rest bleibt.
		$basis = Join-Path $t 'Dashboard\cache'; $c = Join-Path $basis 'target'
		Neu 'Dashboard\cache\target\debug\x.bin' 0; Neu 'Dashboard\anderes\y.bin' 0; Neu 'Dashboard\cache\ohne-tag\x.bin' 0; Neu 'woanders\CACHEDIR.TAG' 0
		Invoke-Cache $c $false -loeschen -basis $basis | Out-Null
		if (-not (Test-Path $c)) { throw 'Selbsttest rot: Cache ohne CACHEDIR.TAG geleert' }
		Neu 'Dashboard\cache\target\CACHEDIR.TAG' 0
		Invoke-Cache (Join-Path $t 'woanders') $false -loeschen -basis $basis | Out-Null
		if (-not (Test-Path (Join-Path $t 'woanders'))) { throw 'Selbsttest rot: Ordner außerhalb der Basis geleert' }
		Invoke-Cache $basis $false -loeschen -basis $basis | Out-Null
		if (-not (Test-Path $basis)) { throw 'Selbsttest rot: Basis selbst geleert' }
		Invoke-Cache $c $true -loeschen -basis $basis | Out-Null
		if (-not (Test-Path $c)) { throw 'Selbsttest rot: Cache trotz laufendem Bau geleert' }
		Invoke-Cache $c $false -loeschen -basis $basis | Out-Null
		if (Test-Path $c) { throw 'Selbsttest rot: Cache nicht geleert' }
		# Sperrdatei des Einspielens: frisch bleibt der Platz, älter als ein Tag (verwaist) wird er geleert.
		Neu 'Dashboard\cache\dienst\CACHEDIR.TAG' 0; Neu 'Dashboard\cache\dienst\EINSPIELEN.lock' 0
		Invoke-Cache (Join-Path $basis 'dienst') $false -loeschen -basis $basis | Out-Null
		if (-not (Test-Path (Join-Path $basis 'dienst\CACHEDIR.TAG'))) { throw 'Selbsttest rot: Cache trotz Sperrdatei geleert' }
		Neu 'Dashboard\cache\dienst\EINSPIELEN.lock' 2
		Invoke-Cache (Join-Path $basis 'dienst') $false -loeschen -basis $basis | Out-Null
		if (Test-Path (Join-Path $basis 'dienst')) { throw 'Selbsttest rot: Cache mit verwaister Sperrdatei nicht geleert' }
		if (-not (Select-String -LiteralPath $Protokoll -Pattern 'verwaist' -Quiet)) { throw 'Selbsttest rot: verwaiste Sperrdatei nicht im Protokoll' }
		if (-not (Test-Path (Join-Path $t 'Dashboard\anderes\y.bin'))) { throw 'Selbsttest rot: Dashboard außerhalb des Caches angefasst' }
		# Alle Plätze: zwei getaggte geleert, ohne Tag und Link bleiben (auch das Ziel des Links).
		Neu 'Dashboard\cache\arbeit-1\CACHEDIR.TAG' 0; Neu 'Dashboard\cache\arbeit-2\CACHEDIR.TAG' 0; Neu 'link-ziel\CACHEDIR.TAG' 0
		New-Item -ItemType Junction -Path (Join-Path $basis 'verlinkt') -Target (Join-Path $t 'link-ziel') | Out-Null
		Invoke-Caches $basis $false -loeschen | Out-Null
		foreach ($weg in 'arbeit-1', 'arbeit-2') { if (Test-Path (Join-Path $basis $weg)) { throw "Selbsttest rot: Cache $weg nicht geleert" } }
		foreach ($bleibt in 'Dashboard\cache\ohne-tag\x.bin', 'Dashboard\cache\verlinkt', 'link-ziel\CACHEDIR.TAG') { if (-not (Test-Path (Join-Path $t $bleibt))) { throw "Selbsttest rot: $bleibt gelöscht" } }
		if (-not (Select-String -LiteralPath $Protokoll -Pattern 'belegt, übersprungen' -Quiet)) { throw 'Selbsttest rot: Protokoll ohne Zeile' }
		'Selbsttest grün'
	} finally { Remove-Item -LiteralPath $t -Recurse -Force -ErrorAction SilentlyContinue }
	return
}

if (-not $Wurzel) { throw '-Wurzel fehlt: der Arbeitsordner der Assistenten (steht in der geplanten Aufgabe). Ohne ihn räumt das Skript nichts auf.' }
if (Test-ZuBreit $Wurzel) { throw "-Wurzel $Wurzel ist zu breit: das Benutzerprofil, ein Anwendungsdaten-Ordner oder ein Ordner darüber." }
# Der Parameter selbst: $tabu oben (gleicher Name, PowerShell unterscheidet nicht) enthält immer das Dashboard.
if ($Scharf -and -not @($PSBoundParameters['Tabu'] | Where-Object { $_ }).Count) { throw '-Tabu fehlt: mit -Scharf sind die Ordner Pflicht, die nie angefasst werden (etwa Verläufe und Daten, die erhalten bleiben sollen).' }
$cacheBasis = Join-Path $env:LOCALAPPDATA 'Dashboard\cache'
"Wurzel: $Wurzel; Bauordner ohne Schreiben seit $Tage Tag(en); Cache-Plätze unter $cacheBasis"
$bautGerade = [bool](Get-Process -Name cargo, rustc -ErrorAction SilentlyContinue)
Invoke-Caches $cacheBasis $bautGerade -loeschen:$Scharf
Invoke-Bereinigen $Wurzel @(Get-Belegt) -loeschen:$Scharf -laufend @(Get-LaufendeSitzungen)
