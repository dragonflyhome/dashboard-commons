/**
 * Fragen beantworten (Etappe A2): die Antwort des Inhabers an den Server schicken.
 *
 * `POST /api/antworten/<projekt>/<id>` ist geschützt (Windows-Anmeldung) und
 * nimmt Antworten nur aus dem Browser. Der Server prüft die Antwort gegen die
 * Frage in fragen.toml, speichert sie und legt einen Vorgang `antwort` an;
 * das wartende Skript des Chats wacht dann auf. Dieselbe Antwort noch einmal
 * ändert nichts (`geaendert: false`); eine andere gibt 409 mit Satz. Bei
 * „wahl“, „ja-nein“ und „bestätigen“ kann der Inhaber eine Anmerkung mitschicken
 * (`notiz`, freiwillig, höchstens `NOTIZ_HOECHSTENS` Zeichen); sie gehört zur
 * Antwort: dieselbe Antwort mit anderer Anmerkung gibt ebenfalls 409. Statt
 * eines Klicks gehen auch eigene Worte („Etwas anderes …“, `etwas-anderes.ts`):
 * `antwort = "etwas-anderes"` mit dem Text in `eigene`, derselbe Weg.
 */
import { API, ApiError, readError } from './api/client';
import type { AnstehendEintrag, AntwortGespeichert, FrageArt } from '$vertrag';

/** Höchstens so viele Zeichen bei einer Antwort als Text (wie im Server). */
export const TEXT_HOECHSTENS = 2000;
/** Höchstens so viele Zeichen bei der freiwilligen Anmerkung (wie im Server). */
export const NOTIZ_HOECHSTENS = 1000;

export const SATZ_GESENDET = 'Gesendet – der Chat ist geweckt.';
/** Steht nach jeder gespeicherten Antwort dabei: Ändern geht nur über den Chat. */
export const SATZ_FALSCH_GEKLICKT = 'Falsch geklickt? Schreib es dem Chat.';
export const SATZ_SCHON_DA = 'Diese Antwort war schon gespeichert.';
export const SATZ_NETZ = 'Die Antwort ließ sich nicht senden. Bitte gleich noch einmal versuchen.';
/** Bei 5xx: der Server hat die Antwort nicht gespeichert (Datenspeicher fehlt, Fehler). */
export const SATZ_NICHT_GESPEICHERT =
	'Deine Antwort ist nicht gespeichert. Bitte später noch einmal.';
export const SATZ_OHNE_ANMELDUNG =
	'Antworten geht nur mit Windows-Anmeldung. Bitte das Dashboard in Microsoft Edge unter http://localhost:8787 öffnen.';

/** Was eine Frage zum Beantworten braucht. */
export type FrageEintrag = Pick<
	AnstehendEintrag,
	'projekt' | 'id' | 'gestellt' | 'titel' | 'frage'
>;

/**
 * Schlüssel einer Frage für die Anzeige: Projekt, id und wann gestellt. Rückt
 * nach einer Antwort die nächste Frage nach (oder kommt dieselbe id neu
 * gestellt), entsteht so ein neuer Baustein mit leeren Feldern; sonst
 * blieben Anmerkung und Text stehen und gingen still mit der nächsten
 * Antwort mit.
 */
export function frageSchluessel(e: Pick<FrageEintrag, 'projekt' | 'id' | 'gestellt'>): string {
	return `${e.projekt}/${e.id}/${e.gestellt ?? ''}`;
}

/**
 * Schickt die Antwort des Inhabers. `antwort` `null` heißt: ohne Wert (bei „bestätigen“);
 * `eigene`: die eigenen Worte bei `antwort = "etwas-anderes"`;
 * `notiz` `null` oder leer: ohne Anmerkung. Wirft `ApiError`.
 */
export async function beantworten(
	e: FrageEintrag,
	antwort: string | null,
	notiz: string | null = null,
	fetchFn: typeof fetch = fetch,
	eigene: string | null = null
): Promise<AntwortGespeichert> {
	const koerper: Record<string, string> = {};
	if (e.gestellt !== null) koerper.gestellt = e.gestellt;
	if (antwort !== null) koerper.antwort = antwort;
	if (notiz !== null && notiz.trim() !== '') koerper.notiz = notiz;
	if (eigene !== null) koerper.eigene = eigene;
	const res = await fetchFn(
		`${API}/antworten/${encodeURIComponent(e.projekt)}/${encodeURIComponent(e.id)}`,
		{
			method: 'POST',
			credentials: 'same-origin',
			headers: { accept: 'application/json', 'content-type': 'application/json' },
			body: JSON.stringify(koerper)
		}
	);
	if (!res.ok) throw await readError(res);
	return (await res.json()) as AntwortGespeichert;
}

/** Der Satz für den Inhaber zu einem Fehler beim Senden. */
export function fehlerSatz(err: unknown): string {
	if (!(err instanceof ApiError)) return SATZ_NETZ;
	if (err.status === 401) return SATZ_OHNE_ANMELDUNG;
	if (err.status >= 500) return SATZ_NICHT_GESPEICHERT;
	return err.message;
}

/** Die Art der Frage in Worten (für Vorlese-Programme und Hinweise). */
export function artWort(a: FrageArt): string {
	switch (a) {
		case 'wahl':
			return 'Auswahl';
		case 'ja-nein':
			return 'Ja oder Nein';
		case 'text':
			return 'freie Antwort';
		case 'bestaetigen':
			return 'bestätigen';
	}
}
