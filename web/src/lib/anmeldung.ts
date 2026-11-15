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

/** Schritt E2: der Satz ohne Anmeldung (wie die 401-Seite des Servers). */
export const SATZ_ANMELDUNG_NOETIG =
	'Anmeldung nötig. Ohne Anmeldung zeigt das Dashboard nur die Anmeldeseite.';

/** Wie man sich hier anmeldet: mit Windows (Edge unter localhost) oder nur mit Passkey. */
export type AnmeldeWeg = 'windows' | 'passkey';

/**
 * Schritt E2: fragt `/api/anmeldung/wer` (öffentlich). 200 heißt Windows,
 * 501 ein anderes System; sonst (kein Server, anderer Code) `null`. Wirft nie.
 */
export async function anmeldeWeg(f: typeof fetch = fetch): Promise<AnmeldeWeg | null> {
	try {
		const res = await f('/api/anmeldung/wer', { headers: { accept: 'application/json' } });
		await res.body?.cancel().catch(() => undefined);
		return res.status === 200 ? 'windows' : res.status === 501 ? 'passkey' : null;
	} catch {
		return null;
	}
}

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
