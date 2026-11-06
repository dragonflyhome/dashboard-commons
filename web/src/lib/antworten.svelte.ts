/**
 * Antworten auf Fragen, einmal für alle Seiten (Etappe A2).
 *
 * Nach dem Senden lädt die Seite neu, und die beantwortete Frage
 * verschwindet (die nächste rückt nach). Darum liegt die Rückmeldung
 * („Gesendet – der Chat ist geweckt“ oder ein Fehler) hier und nicht in der
 * Frage selbst: `AntwortStatus` zeigt sie dort, wo die Frage stand. Eine
 * Erfolgsmeldung verschwindet nach `ZEIGEN_MS` von selbst, ein Fehler bleibt
 * bis zum nächsten Versuch. Nach einer gespeicherten Antwort steht dabei,
 * dass der Inhaber einen Fehlklick dem Chat schreibt (ändern geht nur über ihn).
 *
 * Fokus: Die Frage (und mit ihr der Knopf, auf dem der Fokus lag)
 * verschwindet nach dem Neuladen. Darum setzt `AntwortStatus` den Fokus je
 * Meldung genau einmal (`fokusNehmen`) auf ein festes Ziel des Abschnitts
 * außerhalb der Ansage (Überschrift), nie auf die Meldung selbst. Seit A3
 * (Auflage aus A2) nur, wenn die Antwort aus genau diesem Abschnitt kam und
 * der Fokus noch auf `body` oder dem gesperrten Knopf liegt: Wer inzwischen
 * woanders klickt oder tippt, behält seinen Fokus. Gesperrt bleibt bis nach
 * dem Neuladen, damit beim Nachrücken der nächsten Frage nichts verloren geht,
 * was jemand in der Zwischenzeit tippen wollte.
 */
import {
	beantworten,
	fehlerSatz,
	SATZ_FALSCH_GEKLICKT,
	SATZ_GESENDET,
	SATZ_SCHON_DA,
	type FrageEintrag
} from './antworten.ts';

/** So lange bleibt „Gesendet“ stehen. */
export const ZEIGEN_MS = 15_000;

export interface AntwortMeldung {
	projekt: string;
	id: string;
	/** `gesendet`: gespeichert (oder schon da); `fehler`: nicht gespeichert. */
	art: 'gesendet' | 'fehler';
	satz: string;
	/** Laufende Nummer, damit jede Meldung den Fokus nur einmal bekommt. */
	nr: number;
	/** Aus welchem Abschnitt die Antwort kam (`anstehend`, `start`, `projekt:<slug>`). */
	abschnitt: string;
	/** Bei „Etwas anderes …“ die gesendeten eigenen Worte (für „Deine Antwort: …“), sonst `null`. */
	eigene?: string | null;
	/** Worauf der Fokus beim Senden lag (der Knopf, der dann gesperrt war). */
	ausloeser: Element | null;
}

export class Antworten {
	/** `<projekt>/<id>` der Frage, deren Antwort gerade unterwegs ist. */
	beschaeftigt = $state<string | null>(null);
	meldung = $state<AntwortMeldung | null>(null);

	#fetch: typeof fetch;
	#nr = 0;
	#fokusFuer = 0;
	#uhr: ReturnType<typeof setTimeout> | null = null;

	constructor(fetchFn: typeof fetch = (...a) => fetch(...a)) {
		this.#fetch = fetchFn;
	}

	/** Die Meldung für ein Projekt (`null`: für alle). */
	meldungFuer(projekt: string | null): AntwortMeldung | null {
		const m = this.meldung;
		return m !== null && (projekt === null || m.projekt === projekt) ? m : null;
	}

	/**
	 * `true` genau einmal je Meldung: Dann setzt der Aufrufer den Fokus (auf
	 * sein festes Ziel). Mehrere Anzeigen derselben Meldung holen ihn so nicht hin und her.
	 */
	fokusNehmen(m: AntwortMeldung, abschnitt = ''): boolean {
		if (m.nr === this.#fokusFuer || m.abschnitt !== abschnitt) return false;
		this.#fokusFuer = m.nr;
		const aktiv = typeof document === 'undefined' ? null : document.activeElement;
		return aktiv === null || aktiv === document.body || aktiv === m.ausloeser;
	}

	/**
	 * Schickt die Antwort (mit Anmerkung, wenn `notiz` nicht leer ist; bei
	 * „Etwas anderes …“ mit den eigenen Worten in `eigene`); danach
	 * `danach` (die Seite lädt neu), auch nach einem Fehler: 404 oder 409
	 * heißen, dass sich die Frage geändert hat.
	 */
	async senden(
		e: FrageEintrag,
		antwort: string | null,
		notiz: string | null = null,
		danach?: () => void | Promise<void>,
		abschnitt = '',
		eigene: string | null = null
	): Promise<boolean> {
		this.#uhrAus();
		this.beschaeftigt = `${e.projekt}/${e.id}`;
		this.meldung = null;
		const ausloeser = typeof document === 'undefined' ? null : document.activeElement;
		let gut = false;
		try {
			const r = await beantworten(e, antwort, notiz, this.#fetch, eigene);
			gut = true;
			this.meldung = {
				projekt: e.projekt,
				id: e.id,
				art: 'gesendet',
				satz:
					eigene === null
						? `${r.geaendert ? SATZ_GESENDET : SATZ_SCHON_DA} ${SATZ_FALSCH_GEKLICKT}`
						: r.geaendert
							? SATZ_GESENDET
							: SATZ_SCHON_DA,
				eigene,
				nr: ++this.#nr,
				abschnitt,
				ausloeser
			};
			this.#uhr = setTimeout(() => {
				this.meldung = null;
				this.#uhr = null;
			}, ZEIGEN_MS);
		} catch (err) {
			this.meldung = {
				projekt: e.projekt,
				id: e.id,
				art: 'fehler',
				satz: fehlerSatz(err),
				nr: ++this.#nr,
				abschnitt,
				ausloeser
			};
		}
		try {
			if (danach) await danach();
		} finally {
			// Erst nach dem Neuladen frei (Auflage aus A2).
			this.beschaeftigt = null;
		}
		return gut;
	}

	#uhrAus(): void {
		if (this.#uhr !== null) clearTimeout(this.#uhr);
		this.#uhr = null;
	}
}

export const antworten = new Antworten();
