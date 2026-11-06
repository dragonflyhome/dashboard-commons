/**
 * Regeln für den Baustein `Bild`: welche Dateinamen gehen und wie die
 * Adresse aussieht. Die Namen folgen `is_safe_asset_name` im Server
 * (src/api/projekte.rs), damit das Bild nie an der Regel des Servers scheitert.
 */

/** Endungen, die ein Browser als Bild zeigt (wie `assets.rs`, ohne PDF und Text). */
export const BILD_ENDUNGEN = ['png', 'jpg', 'jpeg', 'webp', 'gif', 'svg'] as const;

/** Windows-Gerätenamen (`con`, `nul.txt`, `com1.png` …) liefert der Server nie aus. */
function istGeraetename(datei: string): boolean {
	const stamm = (datei.split('.')[0] ?? '').toLowerCase();
	return ['con', 'nul', 'prn', 'aux'].includes(stamm) || /^(com|lpt)[1-9]$/.test(stamm);
}

/**
 * Satz, warum `datei` nicht geht, oder `null`. Erlaubt: ein Dateiname aus
 * `assets/` mit A–Z, a–z, 0–9, Punkt, Bindestrich und Unterstrich, nicht mit
 * Punkt vorn, mit Bild-Endung. Kein Pfad, keine Adresse.
 */
export function bildFehler(datei: string): string | null {
	const zeigen = datei.length > 60 ? `${datei.slice(0, 60)}…` : datei;
	if (datei.trim() === '') {
		return 'Bild braucht `datei`: den Namen einer Bild-Datei aus assets/, etwa „foto.jpg“.';
	}
	if (
		datei.length > 128 ||
		!/^[A-Za-z0-9._-]+$/.test(datei) ||
		datei.startsWith('.') ||
		istGeraetename(datei)
	) {
		return `„${zeigen}“ ist kein erlaubter Dateiname. Bild nimmt nur den Namen einer Datei in assets/ (Buchstaben A–Z ohne Umlaute, Ziffern, Punkt, Bindestrich, Unterstrich; nicht mit Punkt vorn), keinen Pfad und keine Adresse.`;
	}
	const endung = datei.includes('.') ? (datei.split('.').pop() ?? '').toLowerCase() : '';
	if (!(BILD_ENDUNGEN as readonly string[]).includes(endung)) {
		return `„${zeigen}“ ist kein Bild. Bild zeigt Dateien mit der Endung ${BILD_ENDUNGEN.map((e) => `.${e}`).join(', ')}.`;
	}
	return null;
}

/** Adresse einer Datei in `projects/<slug>/assets/`, wie der Server sie ausliefert. */
export function bildAdresse(slug: string, datei: string): string {
	return `/projects/${encodeURIComponent(slug)}/assets/${encodeURIComponent(datei)}`;
}
