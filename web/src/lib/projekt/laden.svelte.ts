/**
 * Daten für Projektseiten in neuer Form (Etappe 4v), Import über `$projekt`.
 *
 * Der Rahmen eines Projekts unter `/projects/<slug>` setzt den Projekt-Kontext
 * (`setzeProjekt`); eine Projektseite ruft dann nur
 * `const plan = ladeDaten<Gantt>('gantt')` und bekommt ein Objekt mit
 * Zustand, Inhalt, Abgeleitetem und Fehlersatz. Es lädt neu, wenn der Server
 * das Projekt neu gelesen hat (Live-Kanal `projekt` oder Sammelereignis
 * `inhalt`, Etappe 2d) oder Ereignisse verpasst sein können (`nachholen`).
 */
import { createContext, untrack } from 'svelte';
import type {
	Abgeleitet,
	Ampel,
	Datei,
	DatenAntwort,
	Fluss,
	Fortschritt,
	Fragen,
	Fristen,
	Gantt,
	Kanban,
	Kennzahlen,
	Orte,
	ProjektAntwort,
	Zeitstrahl
} from '$vertrag';
import { API } from '../api/client.ts';
import { betrifftProjekt, live, type InhaltListener } from '../live.svelte.ts';
import { BROWSER_ADRESSE } from '../anmeldung.ts';
import { mitZugang, zugang, type Zugang } from '../zugang.ts';

/** `laedt`: noch nichts da. `fertig`: Inhalt ist da. `fehler`: siehe `satz`. */
export type Zustand = 'laedt' | 'fertig' | 'fehler';

/** Was `ladeDaten` und `ladeKopf` liefern. Alle Felder folgen dem Server von selbst. */
export interface Geladen<T> {
	readonly zustand: Zustand;
	/** Der Inhalt; `null`, solange nichts geladen ist. Nach einem Fehler beim Neuladen bleibt der letzte Stand. */
	readonly inhalt: T | null;
	/** Was der Server aus der Datei rechnet (Anzahl, Erledigtes, Summen, Tage); beim Kopf immer `null`. */
	readonly abgeleitet: Abgeleitet | null;
	/** Bei `fehler`: der Satz des Servers (mit Datei und Zeile, wenn bekannt). */
	readonly satz: string | null;
	/**
	 * Etappe B2: Der Weg verlangt die Windows-Anmeldung (401 oder 403), etwa die
	 * Fragen an den Inhaber. Dann ist `zustand` `fehler`, aber es ist kein Fehler
	 * der Seite; der Baustein sagt nur, dass man sich anmelden muss.
	 */
	readonly ohneAnmeldung?: boolean;
	/** Sofort neu laden (etwa für einen Knopf „neu laden“). */
	neuLaden(): Promise<void>;
}

/** Wovon das Laden abhängt; in Tests ersetzt. */
export interface Umgebung {
	fetch: typeof fetch;
	onInhalt(listener: InhaltListener): () => void;
	onNachholen(listener: () => void): () => void;
	/**
	 * Etappe B2: Etwas an Anstehend des Projekts `slug` hat sich geändert
	 * (Antwort, Einstufung, ein Vorgang des Projekts). Danach kann eine andere
	 * Frage der Warteschlange sichtbar sein; `ladeDaten('fragen')` lädt darum neu.
	 */
	onAnstehend?(slug: string, listener: () => void): () => void;
	/**
	 * Etappe B2: Der Live-Kanal ist angemeldet offen (Anmeldung kam an). Ein
	 * Lader für geschützte Daten, der eben ohne Anmeldung war, lädt dann nach.
	 */
	onAngemeldet?(listener: () => void): () => void;
	/** Der Anmeldestand der Seite (`zugang.ts`); in Tests ersetzt. */
	zugang?: Zugang;
}

/** `change` für Anstehend oder die Aufgaben des Projekts und jeder Vorgang des Projekts. */
function beiAnstehend(slug: string, l: () => void): () => void {
	const aus = [
		live.onChange((c) => {
			const betrifft =
				(c.area === 'anstehend' && (c.path === 'anstehend' || c.path === `anstehend/${slug}`)) ||
				(c.area === 'aufgaben' && c.path === `aufgaben/${slug}`);
			if (betrifft) l();
		}),
		live.onVorgang((v) => {
			if (v.slug === slug) l();
		})
	];
	return () => {
		for (const ab of aus) ab();
	};
}

const standard: Umgebung = {
	fetch: (...args) => fetch(...args),
	onInhalt: (l) => live.onInhalt(l),
	onNachholen: (l) =>
		live.onNachholen(() => {
			l();
		}),
	onAnstehend: beiAnstehend,
	onAngemeldet: (l) =>
		live.onKanal((m) => {
			if (m.art === 'offen') l();
		}),
	zugang
};

/**
 * Etappe B2: Dateien aus `daten/`, deren Weg geschützt ist (nur mit
 * Windows-Anmeldung, `sicherheit::geschuetzt` im Server: `FRAGEN_DATEI`, seit
 * Etappe B3 `FRISTEN_DATEI`).
 * Ohne Anmeldung fragt der Lader sie gar nicht erst an (`mitZugang`), damit
 * eine Seite beim Laden höchstens einen 401 erzeugt; er zeigt gleich den Satz
 * zum Anmelden. `geschuetzt.spec.ts` hält die Liste gleich mit dem Server.
 */
export const GESCHUETZTE_DATEN: readonly string[] = ['fragen', 'fristen'];

/**
 * Dateien, die auch von Anstehend abhängen (Etappe B2: die Warteschlange der
 * Fragen; Etappe B3: die Fristenliste mit Aufgaben und Arbeiten).
 */
export const MIT_ANSTEHEND: readonly string[] = ['fragen', 'fristen'];

// ----- Kontext -----

/** Was die Route über das Projekt weitergibt. */
export interface ProjektKontext {
	readonly slug: string;
}

const [holeKontext, setzeKontext] = createContext<ProjektKontext>();

/**
 * Setzt das Projekt für alle Seiten darunter (nur die Route ruft das).
 * `slug` ist eine Funktion, damit ein Wechsel des Projekts ankommt.
 */
export function setzeProjekt(slug: () => string): void {
	setzeKontext({
		get slug() {
			return slug();
		}
	});
}

/**
 * Das Projekt der Seite, für `ladeDaten`, `ladeKopf` und Bausteine wie
 * `Bild`. Außerhalb einer Projektseite ein Fehler mit Satz, der `wer` nennt.
 */
export function kontext(wer: string): ProjektKontext {
	try {
		return holeKontext();
	} catch {
		throw new Error(
			`${wer} geht nur in einer Projektseite (Seite.svelte oder seiten/<name>.svelte), die das Dashboard unter /projects/<slug> zeigt.`
		);
	}
}

// ----- Laden -----

/** Etappe B2: Daten, die nur mit Anmeldung kommen (etwa die Fragen an den Inhaber). */
export const SATZ_DATEN_OHNE_ANMELDUNG = `Anmelden, um das zu sehen: in Microsoft Edge unter ${BROWSER_ADRESSE}.`;

/** Satz aus einer Fehlerantwort: `fehler` (mit Datei und Zeile), sonst ein eigener. */
async function fehlerSatz(res: Response): Promise<string> {
	try {
		const body = (await res.json()) as { fehler?: unknown };
		if (typeof body.fehler === 'string' && body.fehler !== '') return body.fehler;
	} catch {
		// kein JSON
	}
	if (res.status === 404) return 'Diese Daten gibt es nicht.';
	return `Der Server antwortet mit Fehler ${res.status}.`;
}

/** Ein Ladevorgang mit Runes-Zustand; lädt auf Wunsch neu. */
export class Lader<T> implements Geladen<T> {
	zustand = $state<Zustand>('laedt');
	inhalt = $state.raw<T | null>(null);
	abgeleitet = $state.raw<Abgeleitet | null>(null);
	satz = $state<string | null>(null);
	ohneAnmeldung = $state(false);

	#adresse: string | null = null;
	/** Nummer der letzten Anfrage; ältere Antworten zählen nicht mehr. */
	#nr = 0;
	readonly #auspacken: (json: unknown) => { inhalt: T; abgeleitet: Abgeleitet | null };
	readonly #fetch: typeof fetch;

	constructor(
		auspacken: (json: unknown) => { inhalt: T; abgeleitet: Abgeleitet | null },
		fetchFn: typeof fetch
	) {
		this.#auspacken = auspacken;
		this.#fetch = fetchFn;
	}

	/** Lädt von `adresse`. Neue Adresse: zurück auf `laedt`; dieselbe: der alte Stand bleibt sichtbar. */
	async laden(adresse: string): Promise<void> {
		if (adresse !== this.#adresse) {
			this.#adresse = adresse;
			this.zustand = 'laedt';
			this.inhalt = null;
			this.abgeleitet = null;
			this.satz = null;
		}
		this.#nr += 1;
		const nr = this.#nr;
		let satz: string;
		try {
			const res = await this.#fetch(adresse, { headers: { accept: 'application/json' } });
			if (nr === this.#nr) this.ohneAnmeldung = res.status === 401 || res.status === 403;
			if (res.ok) {
				const { inhalt, abgeleitet } = this.#auspacken(await res.json());
				if (nr !== this.#nr) return;
				this.inhalt = inhalt;
				this.abgeleitet = abgeleitet;
				this.satz = null;
				this.zustand = 'fertig';
				return;
			}
			satz = this.ohneAnmeldung ? SATZ_DATEN_OHNE_ANMELDUNG : await fehlerSatz(res);
		} catch {
			satz = 'Der Server antwortet gerade nicht. Die Seite lädt neu, sobald er wieder da ist.';
		}
		if (nr !== this.#nr) return;
		this.satz = satz;
		this.zustand = 'fehler';
	}

	neuLaden(): Promise<void> {
		return this.#adresse === null ? Promise.resolve() : this.laden(this.#adresse);
	}
}

/**
 * Lädt `adresse(slug)` und danach, wenn der Server dieses Projekt neu gelesen
 * hat (oder viele auf einmal), und beim Nachholen; mit `mitAnstehend` auch,
 * wenn sich Anstehend des Projekts ändert. Mit `geschuetzt` (Etappe B2) geht
 * ohne Anmeldung keine Anfrage hinaus, und nach der Anmeldung lädt er nach. Muss beim Anlegen einer Komponente
 * laufen (oder in `$effect.root`), weil es einen `$effect` anlegt; der räumt
 * beim Abbau auf.
 */
export function beobachten<T>(
	slug: () => string,
	adresse: (slug: string) => string,
	auspacken: (json: unknown) => { inhalt: T; abgeleitet: Abgeleitet | null },
	umgebung: Umgebung,
	mitAnstehend = false,
	geschuetzt = false
): Lader<T> {
	// Geschützt: ohne Anmeldung keine Anfrage, die Antwort des Servers kommt lokal (401).
	const holen = geschuetzt ? mitZugang(umgebung.fetch, umgebung.zugang ?? zugang) : umgebung.fetch;
	const lader = new Lader<T>(auspacken, holen);
	$effect(() => {
		const s = slug();
		const url = adresse(s);
		untrack(() => void lader.laden(url));
		const aus = umgebung.onInhalt((a) => {
			if (betrifftProjekt(a, s)) void lader.laden(url);
		});
		const ausNachholen = umgebung.onNachholen(() => void lader.laden(url));
		const ausAnstehend =
			mitAnstehend && umgebung.onAnstehend
				? umgebung.onAnstehend(s, () => void lader.laden(url))
				: () => undefined;
		const ausAngemeldet =
			geschuetzt && umgebung.onAngemeldet
				? umgebung.onAngemeldet(() => {
						if (lader.ohneAnmeldung) void lader.laden(url);
					})
				: () => undefined;
		return () => {
			aus();
			ausNachholen();
			ausAnstehend();
			ausAngemeldet();
		};
	});
	return lader;
}

function datenAuspacken(json: unknown): { inhalt: unknown; abgeleitet: Abgeleitet | null } {
	const a = json as DatenAntwort;
	return { inhalt: a.inhalt, abgeleitet: a.abgeleitet };
}

/** Adresse einer Datei aus `daten/`. */
export function datenAdresse(slug: string, name: string): string {
	return `${API}/projekte/${encodeURIComponent(slug)}/daten/${encodeURIComponent(name)}`;
}

/** Adresse des Kopfs. */
export function kopfAdresse(slug: string): string {
	return `${API}/projekte/${encodeURIComponent(slug)}`;
}

/** Die Dateien der Katalog-Bausteine und ihr Typ:  ist dann schon richtig typisiert. */
export interface DatenTypen {
	kennzahlen: Kennzahlen;
	ampel: Ampel;
	fortschritt: Fortschritt;
	zeitstrahl: Zeitstrahl;
	gantt: Gantt;
	kanban: Kanban;
	fluss: Fluss;
	fragen: Fragen;
	orte: Orte;
	fristen: Fristen;
}

/**
 * Eine Datei aus `daten/` des eigenen Projekts: `ladeDaten('gantt')` holt
 * `/api/projekte/<slug>/daten/gantt`. Für die Dateien der Katalog-Bausteine
 * folgt der Typ aus dem Namen (`DatenTypen`); für eigene Daten nennt die
 * Seite ihn selbst: `ladeDaten<Notizen>('eigen-notizen')`. 404 und 422
 * ergeben `zustand = 'fehler'` mit dem Satz des Servers.
 */
export function ladeDaten<N extends keyof DatenTypen>(
	name: N,
	umgebung?: Umgebung
): Geladen<Datei<DatenTypen[N]>>;
export function ladeDaten<T = unknown>(name: string, umgebung?: Umgebung): Geladen<T>;
export function ladeDaten(name: string, umgebung: Umgebung = standard): Geladen<unknown> {
	const k = kontext('ladeDaten');
	return beobachten<unknown>(
		() => k.slug,
		(s) => datenAdresse(s, name),
		datenAuspacken,
		umgebung,
		MIT_ANSTEHEND.includes(name),
		GESCHUETZTE_DATEN.includes(name)
	);
}

/** Der Kopf des eigenen Projekts (`projekt.toml`, Namen in `daten/`). */
export function ladeKopf(umgebung: Umgebung = standard): Geladen<ProjektAntwort> {
	const k = kontext('ladeKopf');
	return beobachten<ProjektAntwort>(
		() => k.slug,
		kopfAdresse,
		(json) => ({ inhalt: json as ProjektAntwort, abgeleitet: null }),
		umgebung
	);
}
