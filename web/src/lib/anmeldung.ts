/**
 * Windows-Anmeldung (Etappe 7b): Chromium meldet nur auf `localhost` von
 * selbst an, auf 127.0.0.1 nicht. Hier stehen die Sätze und die Adresse,
 * unter der dieselbe Seite angemeldet geht.
 */

/** Die Adresse des Dashboards im Browser. */
export const BROWSER_ADRESSE = 'http://localhost:8787';

export const SATZ_FREMDES_KONTO = 'Dieses Windows-Konto darf hier nicht.';
export const SATZ_NUR_ANGEMELDET =
	'Diese Werte zeigt das Dashboard nur nach Windows-Anmeldung (über localhost).';

/** Etappe 4d (Abnahme): ohne Anmeldung auf /entscheidungen und /zielbild, wie bei /anstehend. */
export const SATZ_ENTSCHEIDUNGEN_ANMELDEN = `Die Entscheidungen siehst du in Microsoft Edge unter ${BROWSER_ADRESSE}`;
export const SATZ_ZIELBILD_ANMELDEN = `Das Zielbild siehst du in Microsoft Edge unter ${BROWSER_ADRESSE}`;
/** Etappe 6b: ohne Anmeldung auf /anleitung, wie beim Zielbild. */
export const SATZ_ANLEITUNG_ANMELDEN = `Die Anleitung siehst du in Microsoft Edge unter ${BROWSER_ADRESSE}`;

/**
 * Dieselbe Seite unter `localhost`: Host ersetzt, Port, Pfad, Suche und
 * Anker bleiben. Eine unlesbare Adresse ergibt die Startadresse.
 */
export function localhostAdresse(href: string): string {
	try {
		const u = new URL(href);
		u.hostname = 'localhost';
		return u.href;
	} catch {
		return `${BROWSER_ADRESSE}/`;
	}
}
