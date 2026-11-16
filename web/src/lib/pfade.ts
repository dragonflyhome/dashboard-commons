/**
 * Lokale Pfade in der Oberfläche (Prüfbericht: Links-Kästen setzten
 * lokale Pfade als Links, die im Browser nicht aufgehen).
 *
 * Ein lokaler Pfad (`C:\…`, `C:/…`, `file:…`, `\\rechner\…`, unter macOS
 * und Linux `/Users/<name>`, `/home/<name>`, `/Volumes/<x>` und die Ordner
 * des Systems) ist nie ein Link. Öffnen geht nur über „Orte“
 * (`daten/orte.toml`, Knopf „Öffnen“, Entscheidung lokal-oeffnen). Angemeldet
 * zeigt die Oberfläche jeden Pfad im Klartext (Schritt E3); geschwärzt wird nur
 * im Server, auf öffentlichen Wegen und in Fehlerantworten (`src/datenschutz.rs`).
 */

/** Hinweis neben einem lokalen Pfad in einem Links-Kasten. */
export const SATZ_LOKALER_PFAD =
	'Lokaler Ort: geht im Browser nicht auf. Der Chat kann ihn in daten/orte.toml eintragen, dann steht hier „Öffnen“.';

/**
 * Ordner direkt unter der Wurzel, die unter macOS und Linux ein lokaler Ort
 * sind: Profile und System. Alles andere mit `/` vorn ist ein Weg im
 * Dashboard (`/projects/x`, `/pc`) und bleibt ein Link.
 */
const POSIX_WURZELN = [
	'Users',
	'home',
	'root',
	'Volumes',
	'System',
	'Library',
	'Applications',
	'private',
	'var',
	'tmp',
	'opt',
	'usr',
	'etc',
	'mnt',
	'media',
	'run',
	'srv'
];
const POSIX_LOKAL = new RegExp(`^/(?:${POSIX_WURZELN.join('|')})(?:/|$)`);

/**
 * `C:\…`, `C:/…`, `file:…`, ein Netzwerkpfad `\\…` oder ein Pfad in der Form
 * von macOS und Linux unter einer Profil- oder Systemwurzel (`/Users/<name>`,
 * `/Volumes/<x>`, `/usr/bin`). Ein Weg im Dashboard mit `/` vorn ist nie lokal.
 */
export function istLokalerPfad(adresse: string): boolean {
	const a = adresse.trim();
	return (
		/^[a-z]:[\\/]/i.test(a) || /^file:/i.test(a) || a.startsWith('\\\\') || POSIX_LOKAL.test(a)
	);
}
