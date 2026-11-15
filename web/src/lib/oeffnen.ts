/**
 * Lokale Dateien und Ordner öffnen (Entscheidung lokal-oeffnen).
 *
 * Browser öffnen keine `file:`-Adressen aus einer http-Seite, und das bleibt
 * so. Stattdessen schickt der Knopf „Öffnen“ neben einem Ort aus
 * `daten/orte.toml` `POST /api/oeffnen` mit Projekt und Kennung, nie einen
 * Pfad. Der Server schlägt den Pfad nach, prüft ihn, und der Bote öffnet ihn
 * in deiner Windows-Sitzung (Datei mit dem zugeordneten Programm, Ordner im
 * Explorer). Die Antwort ist nur ein Satz.
 */
import { API, ApiError, readError, SATZ_KEINE_VERBINDUNG } from './api/client.ts';
import type { OeffnenAntwort, OrteVoll, OrtVoll } from '$vertrag';
import { mitZugang } from './zugang.ts';

export const SATZ_GEOEFFNET = 'Geöffnet.';
export const SATZ_OHNE_ANMELDUNG =
	'Öffnen geht nur mit Windows-Anmeldung. Bitte das Dashboard in Microsoft Edge unter http://localhost:8787 öffnen.';

/** Schickt Projekt und Kennung (sonst nichts). Wirft `ApiError` mit dem Satz des Servers. */
export async function oeffnen(
	projekt: string,
	kennung: string,
	fetchFn: typeof fetch = fetch
): Promise<OeffnenAntwort> {
	const res = await fetchFn(`${API}/oeffnen`, {
		method: 'POST',
		credentials: 'same-origin',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify({ projekt, kennung })
	});
	if (!res.ok) throw await readError(res);
	return (await res.json()) as OeffnenAntwort;
}

/** Der Satz zu einem Fehler: ohne Anmeldung ein eigener, sonst der des Servers. */
export function fehlerSatz(err: unknown): string {
	if (!(err instanceof ApiError)) return SATZ_KEINE_VERBINDUNG;
	if (err.status === 401) return SATZ_OHNE_ANMELDUNG;
	return err.message;
}

/**
 * Holt die Orte eines Projekts mit Pfad (`GET /api/projekte/<slug>/orte`,
 * geschützt, Schritt E1b); `null` ohne Anmeldung oder bei Fehlern. Wirft nie.
 * `fetchFn` geht durch `mitZugang`: ohne Anmeldung keine Anfrage ins Netz.
 */
export async function ladeOrte(
	projekt: string,
	fetchFn: typeof fetch = fetch
): Promise<OrteVoll | null> {
	try {
		const res = await mitZugang(fetchFn)(`${API}/projekte/${encodeURIComponent(projekt)}/orte`, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
		if (!res.ok) {
			await res.body?.cancel().catch(() => undefined);
			return null;
		}
		const o = (await res.json()) as Partial<OrteVoll> | null;
		return o !== null && Array.isArray(o.orte) ? (o as OrteVoll) : null;
	} catch {
		return null;
	}
}

/**
 * Die `file:`-Adresse zu einem geprüften Ort, sonst `null`: nur mit Laufwerk
 * (`C:\…`), nie ein UNC- oder Gerätepfad (zweite Sicherung neben `geprueft`
 * vom Server). Jeder Teil prozentkodiert. Geöffnet wird nie über diese
 * Adresse, nur über `oeffnen` (der Baustein fängt Klicks ab).
 */
export function dateiAdresse(ort: Pick<OrtVoll, 'geprueft' | 'pfad'>): string | null {
	const p = ort.pfad;
	if (!ort.geprueft || p === null || !/^[a-z]:[\\/]/i.test(p)) return null;
	const [laufwerk, ...rest] = p.replaceAll('\\', '/').split('/');
	return `file:///${laufwerk ?? ''}/${rest.map(encodeURIComponent).join('/')}`;
}
