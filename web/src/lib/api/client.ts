import { error } from '@sveltejs/kit';
import type {
	AnleitungAntwort,
	ArchivListe,
	DatenBericht,
	EntscheidungsListe,
	KeyList,
	PcBenutzer,
	PcReport,
	PcVerlaufAntwort,
	ProjectDetail,
	ProjectList,
	ProjektAntwort,
	PruefungProjekt,
	SicherungAntwort,
	Verlauf,
	VorgangDetail,
	VorgangsUebersicht,
	ZielbildAntwort
} from '$vertrag';
import { abrufen } from './antwortet.svelte.ts';

/** Die API liegt bei der Wurzel des Rust-Servers, neben der Oberfläche. */
export const API = '/api';

export class ApiError extends Error {
	constructor(
		public readonly status: number,
		message: string,
		/** Festes Wort aus der Fehlerantwort (`art`), etwa `fremdes-konto`; sonst `null`. */
		public readonly art: string | null = null
	) {
		super(message);
		this.name = 'ApiError';
	}
}

/**
 * Der Server verlangt die Windows-Anmeldung (401 mit `art: "anmeldung"`).
 * Im Browser passiert das, wenn das Dashboard über 127.0.0.1 statt über
 * localhost geöffnet ist; nur auf localhost meldet Windows von selbst an.
 */
export class AnmeldungNoetig extends ApiError {
	constructor(message = 'Bitte mit dem Windows-Konto anmelden.') {
		super(401, message, 'anmeldung');
		this.name = 'AnmeldungNoetig';
	}
}

/** Fehlerantwort des Rust-Servers: `fehler` in einfachem Deutsch, `art` als festes Wort. */
interface ErrorBody {
	fehler?: unknown;
	art?: unknown;
}

/** Ohne Verbindung zum Dashboard (fetch wirft, etwa weil der Dienst neu startet). */
export const SATZ_KEINE_VERBINDUNG =
	'Keine Verbindung zum Dashboard. Bitte gleich noch einmal versuchen.';

/**
 * Ein Satz in einfachem Deutsch zu einem HTTP-Status, wenn die Antwort keinen
 * eigenen Satz mitbringt (kein JSON, etwa „413 Payload Too Large“ von axum
 * oder eine Fehlerseite davor).
 */
export function statusSatz(status: number): string {
	switch (status) {
		case 400:
			return 'Die Anfrage war fehlerhaft.';
		case 401:
			return 'Bitte mit dem Windows-Konto anmelden.';
		case 403:
			return 'Das ist von hier aus nicht erlaubt.';
		case 404:
			return 'Das gibt es nicht (mehr).';
		case 409:
			return 'Das hat sich inzwischen geändert.';
		case 413:
			return 'Das ist zu lang zum Senden.';
		case 422:
			return 'Das ließ sich nicht verarbeiten. Bitte die Eingabe prüfen.';
		case 429:
			return 'Gerade kommen zu viele Anfragen. Bitte kurz warten und noch einmal versuchen.';
		case 500:
			return 'Im Dashboard ist ein Fehler passiert. Bitte später noch einmal.';
		case 502:
		case 503:
		case 504:
			return 'Das Dashboard ist gerade nicht erreichbar. Bitte später noch einmal.';
		default:
			return `Etwas ist schiefgegangen (Fehler ${status}).`;
	}
}

/** Der Satz zu einem Fehler aus `getJson`, `postJson` und Co.: der des Servers, sonst ohne Verbindung. */
export function fehlerText(err: unknown): string {
	return err instanceof ApiError ? err.message : SATZ_KEINE_VERBINDUNG;
}

/**
 * Netzfehler des Browsers („Failed to fetch“ in Chrome und Edge,
 * „NetworkError when attempting to fetch resource.“ in Firefox, „Load failed“
 * in Safari) auf Deutsch (Auflage aus A2): Der Dienst startet etwa gerade neu.
 */
export function istNetzfehler(error: unknown): boolean {
	return (
		error instanceof TypeError &&
		/failed to fetch|networkerror|load failed|network request failed/i.test(error.message)
	);
}

/**
 * Für `load`: ein `ApiError` wird ein HttpError mit Code, Satz und `art`, ein
 * Netzfehler 503 „keine Verbindung“; anderes fliegt unverändert weiter. Bei
 * einem Fehler, der kein HttpError ist, fragt SvelteKit 3.1 `updated.check()`
 * und lädt bei neuer Fassung nativ neu, ohne beforeNavigate, also auch über
 * eine offene Eingabe hinweg.
 */
export function alsLadeFehler(e: unknown): never {
	if (e instanceof ApiError) error(e.status, e.message, e.art === null ? {} : { art: e.art });
	if (istNetzfehler(e)) error(503, SATZ_KEINE_VERBINDUNG);
	throw e;
}

/** Liest die Fehlerantwort und baut den passenden Fehler (Satz ohne JSON: `statusSatz`). */
export async function readError(res: Response): Promise<ApiError> {
	let message = statusSatz(res.status);
	let art: string | null = null;
	try {
		const body = (await res.json()) as ErrorBody;
		if (typeof body.fehler === 'string' && body.fehler.trim() !== '') message = body.fehler;
		if (typeof body.art === 'string') art = body.art;
	} catch {
		// kein JSON, dann der Satz zum Status
	}
	if (res.status === 401 && art === 'anmeldung') return new AnmeldungNoetig(message);
	return new ApiError(res.status, message, art);
}

/**
 * Holt JSON und wirft bei Fehlern einen `ApiError`. `fetchFn` ist das
 * `fetch` aus SvelteKit-`load`, damit Aufrufe dort richtig verfolgt werden.
 */
export async function getJson<T>(path: string, fetchFn: typeof fetch = fetch): Promise<T> {
	const res = await abrufen(fetchFn, path, { headers: { accept: 'application/json' } });
	if (!res.ok) throw await readError(res);
	return (await res.json()) as T;
}

export const api = {
	projects: (f?: typeof fetch) => getJson<ProjectList>(`${API}/projects`, f),
	/** Entscheidung archiv-status: die archivierten Projekte (Archivansicht). */
	archiv: (f?: typeof fetch) => getJson<ArchivListe>(`${API}/archiv`, f),
	project: (slug: string, f?: typeof fetch) =>
		getJson<ProjectDetail>(`${API}/projects/${encodeURIComponent(slug)}`, f),
	pc: (f?: typeof fetch) => getJson<PcReport>(`${API}/pc`, f),
	/** Geschützt: ohne Windows-Anmeldung 401 (`AnmeldungNoetig`). */
	pcBenutzer: (f?: typeof fetch) => getJson<PcBenutzer>(`${API}/pc/benutzer`, f),
	/**
	 * Verlauf des PCs (Etappe 2e): Ereignisse ohne Reihenfolge, dazu Hinweise.
	 * Ohne `seit` die letzten 90 Tage. Frei; Starts, Meldungen und Vorgänge nur,
	 * wenn die Verbindung zufällig schon angemeldet ist. Die Seiten nehmen
	 * `verlaufVoll` und diesen Weg nur, wenn die Anmeldung nicht geht.
	 */
	verlauf: (seit?: string, f?: typeof fetch) =>
		getJson<Verlauf>(
			`${API}/pc/verlauf${seit === undefined ? '' : `?seit=${encodeURIComponent(seit)}`}`,
			f
		),
	/**
	 * Hotfix 3a-1: derselbe Verlauf, immer mit Starts, Meldungen und Vorgängen.
	 * Geschützt: ohne Anmeldung 401 mit `WWW-Authenticate: Negotiate`, erst
	 * darauf schickt Edge die Windows-Anmeldung mit.
	 */
	verlaufVoll: (seit?: string, f?: typeof fetch) =>
		getJson<Verlauf>(
			`${API}/pc/verlauf/voll${seit === undefined ? '' : `?seit=${encodeURIComponent(seit)}`}`,
			f
		),
	keys: (f?: typeof fetch) => getJson<KeyList>(`${API}/schluessel`, f),
	daten: (f?: typeof fetch) => getJson<DatenBericht>(`${API}/daten`, f),
	pcVerlauf: (stunden: number, f?: typeof fetch) =>
		getJson<PcVerlaufAntwort>(`${API}/daten/pc-verlauf?stunden=${encodeURIComponent(stunden)}`, f),
	sichern: (f?: typeof fetch) => postJson<SicherungAntwort>(`${API}/daten/sichern`, f),
	vorgaengeUebersicht: (f?: typeof fetch) =>
		getJson<VorgangsUebersicht>(`${API}/vorgaenge/uebersicht`, f),
	vorgang: (id: string, f?: typeof fetch) =>
		getJson<VorgangDetail>(`${API}/vorgaenge/${encodeURIComponent(id)}`, f),
	/** Neue Form (Etappe 4v): Kopf aus projekt.toml, Vorschau, Namen in daten/. */
	projekt: (slug: string, f?: typeof fetch) =>
		getJson<ProjektAntwort>(`${API}/projekte/${encodeURIComponent(slug)}`, f),
	/** Befunde der Prüfung für ein Projekt. */
	pruefung: (slug: string, f?: typeof fetch) =>
		getJson<PruefungProjekt>(`${API}/pruefung/${encodeURIComponent(slug)}`, f),
	/** System-Seite /entscheidungen (Etappe 4d): docs/entscheidungen/*.toml, neueste zuerst. */
	entscheidungen: (f?: typeof fetch) => getJson<EntscheidungsListe>(`${API}/entscheidungen`, f),
	/** System-Seite /zielbild (Etappe 4d): docs/zielbild/*.toml; leer, solange nichts da ist. */
	zielbild: (f?: typeof fetch) => getJson<ZielbildAntwort>(`${API}/zielbild`, f),
	/** System-Seite /anleitung (Etappe 6b): docs/anleitung/*.toml im Code-Repo, geschützt. */
	anleitung: (f?: typeof fetch) => getJson<AnleitungAntwort>(`${API}/anleitung`, f)
};

/**
 * Schickt einen POST ohne Inhalt und liest JSON. Der Browser setzt `Origin`
 * selbst; der Rust-Server prüft ihn. Fehler wie bei `getJson`.
 */
export async function postJson<T>(path: string, fetchFn: typeof fetch = fetch): Promise<T> {
	const res = await abrufen(fetchFn, path, {
		method: 'POST',
		credentials: 'same-origin',
		headers: { accept: 'application/json' }
	});
	if (!res.ok) throw await readError(res);
	return (await res.json()) as T;
}

/** Adresse für den Export des Datenspeichers (TOML zum Ansehen, JSON zum Herunterladen). */
export function exportUrl(format: 'toml' | 'json'): string {
	return `${API}/daten/export?format=${format}`;
}
