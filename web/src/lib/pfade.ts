/**
 * Lokale Pfade in der Oberfläche (Prüfbericht: Links-Kästen setzten
 * lokale Pfade als Links, die im Browser nicht aufgehen; /pc zeigte rohe
 * Adressen mit dem Benutzernamen).
 *
 * - Ein lokaler Pfad (`C:\…`, `C:/…`, `file:…`, `\\rechner\…`) ist nie ein
 *   Link. Öffnen geht nur über „Orte“ (`daten/orte.toml`, Knopf „Öffnen“,
 *   Entscheidung lokal-oeffnen).
 * - Der Teil `C:\Users\<name>` (auch „Benutzer“, auch mit `/`) wird beim
 *   Zeigen zu `%USERPROFILE%`: der Name des Kontos steht nirgends im Bild.
 */

/** Hinweis neben einem lokalen Pfad in einem Links-Kasten. */
export const SATZ_LOKALER_PFAD =
	'Lokaler Ort: geht im Browser nicht auf. Der Chat kann ihn in daten/orte.toml eintragen, dann steht hier „Öffnen“.';

/** Platzhalter für das Benutzerprofil. */
export const PROFIL_PLATZHALTER = '%USERPROFILE%';

/** `C:\…`, `C:/…`, `file:…` oder ein Netzwerkpfad `\\…`. */
export function istLokalerPfad(adresse: string): boolean {
	const a = adresse.trim();
	return /^[a-z]:[\\/]/i.test(a) || /^file:/i.test(a) || a.startsWith('\\\\');
}

/**
 * Ersetzt jedes Benutzerprofil (`<Laufwerk>:\Users\<name>` oder
 * `…\Benutzer\<name>`, auch mit `/` und als `file:///`) durch `%USERPROFILE%`.
 * Der Rest des Pfads bleibt, damit man den Ort noch erkennt.
 */
export function profilVerbergen(text: string): string {
	return text.replace(
		/(file:\/{2,3})?[a-z]:[\\/]+(?:users|benutzer)[\\/]+[^\\/"'<>|:*?\r\n]+/gi,
		PROFIL_PLATZHALTER
	);
}
