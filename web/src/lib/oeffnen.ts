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
import type { OeffnenAntwort } from '$vertrag';

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
