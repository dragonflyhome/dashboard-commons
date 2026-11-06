/**
 * Die offenen Aufgaben für den Inhaber, einmal für alle Seiten.
 *
 * Zwei Wege: Die Zahl im Seitenkopf kommt aus `/api/aufgaben/anzahl` (der
 * Server zählt, ohne einen Link zu entschlüsseln) und wird immer geladen. Die
 * volle Liste mit den Links (`/api/aufgaben?offen=1`) lädt nur, solange ein
 * Kasten sie zeigt: Startseite „Für dich“ und Projektseiten melden sich mit
 * `benutzen()` an und beim Verlassen wieder ab. Live: ein `change` mit
 * Bereich `aufgaben` oder ein Nachholen lädt die Zahl neu, die Liste nur,
 * wenn jemand sie benutzt. Um Mitternacht (lokal) lädt beides neu: die
 * Frist-Stufe jeder Aufgabe rechnet der Server am Tag der Anfrage.
 */
import { live } from './live.svelte.ts';
import { beiTageswechsel } from './tageswechsel.ts';
import { abhaken, ladeAnzahl, ladeAufgaben, type AufgabenStand } from './aufgaben.ts';
import { ApiError } from './api/client';
import { ETWAS_ANDERES } from './etwas-anderes.ts';
import { mitZugang } from './zugang.ts';
import type { AufgabeAnsicht, ChangeEvent } from '$vertrag';

/**
 * Entscheidung archiv-status: Ändert sich der Status eines Projekts
 * (archiviert oder zurück), ändern sich Liste und Zahl der Aufgaben, ohne
 * dass eine Aufgabe sich ändert. Der Server meldet das als geänderten Kopf
 * (`projects/<slug>/projekt.toml`) oder, für ein archiviertes
 * Projekt, als `anstehend` ohne Projekt.
 */
export function statusKannSichAendern(c: ChangeEvent): boolean {
	if (c.area === 'anstehend') return c.path === 'anstehend';
	return c.area === 'projects' && /^projects\/[^/]+\/projekt\.toml$/.test(c.path);
}

export class Aufgaben {
	stand = $state<AufgabenStand>('laedt');
	liste = $state<AufgabeAnsicht[]>([]);
	/** Zahl der offenen Aufgaben (aus `/api/aufgaben/anzahl`). */
	zahl = $state(0);
	/** Satz für den Inhaber, wenn die Liste nicht da ist (ohne Anmeldung, Fehler). */
	fehler = $state<string | null>(null);
	/** Satz für den Inhaber, wenn der letzte Haken scheiterte. */
	hakenFehler = $state<string | null>(null);
	/** Kennung, deren Haken gerade läuft. */
	beschaeftigt = $state<string | null>(null);

	#laeuft: Promise<void> | null = null;
	#nochmal = false;
	/** Wie viele Kästen die Liste gerade zeigen. */
	#nutzer = 0;
	/** Nummer der letzten Zählung; ältere Antworten zählen nicht mehr. */
	#zaehlung = 0;
	#fetch: typeof fetch;

	constructor(fetchFn: typeof fetch = mitZugang((...a) => fetch(...a))) {
		this.#fetch = fetchFn;
	}

	/** Was der Kasten als Satz zeigt: ohne Liste der Grund, sonst ein gescheiterter Haken. */
	get meldung(): string | null {
		return this.stand === 'da' ? this.hakenFehler : this.fehler;
	}

	/** Zahl der offenen Aufgaben (für den Seitenkopf). */
	get anzahl(): number {
		return this.zahl;
	}

	/**
	 * Entscheidung archiv-status: Die Aufgaben archivierter Projekte
	 * fehlen in `liste`; ihre Seite holt sie mit `ladenFuer` hierher.
	 */
	archiv = $state<Record<string, AufgabeAnsicht[]>>({});

	/** Die offenen Aufgaben eines Projekts; bei einem archivierten aus `archiv`. */
	fuer(projekt: string, archiviert = false): AufgabeAnsicht[] {
		if (archiviert) return this.archiv[projekt] ?? [];
		return this.liste.filter((a) => a.projekt === projekt);
	}

	/**
	 * Für die Seite eines archivierten Projekts: holt nur dessen offene
	 * Aufgaben (`/api/aufgaben?offen=1&projekt=`). Geht das nicht, bleibt es
	 * beim Stand der Liste.
	 */
	async ladenFuer(projekt: string): Promise<void> {
		const l = await ladeAufgaben(this.#fetch, projekt);
		if (l.stand === 'da') this.archiv = { ...this.archiv, [projekt]: l.aufgaben };
	}

	/** Ob gerade ein Kasten die Liste zeigt. */
	get benutzt(): boolean {
		return this.#nutzer > 0;
	}

	/** Kam während des Ladens ein weiterer Wunsch? */
	#mehr(): boolean {
		return this.#nochmal;
	}

	/** Lädt nur die Zahl (ohne Links). Ohne Anmeldung oder bei Fehlern 0. */
	async zaehlen(): Promise<void> {
		const nummer = ++this.#zaehlung;
		const n = await ladeAnzahl(this.#fetch);
		if (nummer === this.#zaehlung) this.zahl = n ?? 0;
	}

	/** Lädt die Liste neu; kommt währenddessen noch ein Wunsch, folgt genau ein weiterer Lauf. */
	laden(): Promise<void> {
		if (this.#laeuft !== null) {
			this.#nochmal = true;
			return this.#laeuft;
		}
		this.#laeuft = (async () => {
			do {
				this.#nochmal = false;
				const l = await ladeAufgaben(this.#fetch);
				this.stand = l.stand;
				this.liste = l.aufgaben;
				this.fehler = l.fehler;
			} while (this.#mehr());
		})().finally(() => {
			this.#laeuft = null;
		});
		return this.#laeuft;
	}

	/**
	 * Ein Kasten zeigt die Liste: beim ersten lädt sie. Gibt die Abmeldung
	 * zurück (passt als Rückgabe von `$effect`).
	 */
	benutzen(): () => void {
		this.#nutzer += 1;
		if (this.#nutzer === 1) void this.laden();
		let ab = false;
		return () => {
			if (ab) return;
			ab = true;
			this.#nutzer -= 1;
		};
	}

	/** Haken des Inhabers (oder ein Knopf mit Antwort); danach Liste und Zahl neu. */
	async haken(id: string, antwort: string | null = null): Promise<void> {
		this.beschaeftigt = id;
		this.hakenFehler = null;
		try {
			await abhaken(id, antwort, this.#fetch);
		} catch (e) {
			this.hakenFehler = e instanceof ApiError ? e.message : 'Der Haken ließ sich nicht speichern.';
		} finally {
			this.beschaeftigt = null;
		}
		await Promise.all([
			this.laden(),
			this.zaehlen(),
			...Object.keys(this.archiv).map((p) => this.ladenFuer(p))
		]);
	}

	/**
	 * „Etwas anderes …“: eigene Worte statt eines
	 * Klicks, derselbe Weg wie der Haken. Die Aufgabe bleibt offen; der Chat
	 * bekommt den Text als Rückmeldung. Gibt den Satz zurück, wenn es nicht
	 * ging (er steht dann im Feld), sonst `null`; danach Liste und Zahl neu.
	 */
	async rueckmelden(id: string, eigene: string): Promise<string | null> {
		this.beschaeftigt = id;
		this.hakenFehler = null;
		let satz: string | null = null;
		try {
			await abhaken(id, ETWAS_ANDERES, this.#fetch, eigene);
		} catch (e) {
			satz = e instanceof ApiError ? e.message : 'Deine Antwort ließ sich nicht senden.';
		} finally {
			this.beschaeftigt = null;
		}
		await Promise.all([this.laden(), this.zaehlen()]);
		return satz;
	}

	/** Nach einer Änderung: die Zahl immer, die Liste nur, wenn ein Kasten sie zeigt. */
	#neu(): void {
		void this.zaehlen();
		if (this.benutzt) void this.laden();
	}

	/** Erste Zählung und Live-Kanal; gibt die Abmeldung zurück. */
	start(): () => void {
		void this.zaehlen();
		const offChange = live.onChange((c) => {
			if (c.area === 'aufgaben' || statusKannSichAendern(c)) this.#neu();
		});
		const offNachholen = live.onNachholen(() => {
			this.#neu();
		});
		// Etappe 3a+: um Mitternacht neu laden, damit die Frist-Stufen des Servers stimmen.
		const offTag = beiTageswechsel(() => {
			this.#neu();
		});
		return () => {
			offChange();
			offNachholen();
			offTag();
		};
	}
}

export const aufgaben = new Aufgaben();
