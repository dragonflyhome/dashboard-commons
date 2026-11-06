/**
 * „Anstehend“ einmal für alle Seiten (Etappe A1): die Zahl der Stufe Jetzt
 * am Reiter und die Liste für die Seite /anstehend kommen aus derselben
 * Abfrage (`/api/anstehend` ohne Filter). Live: ein `change` mit Bereich
 * `anstehend` oder `aufgaben` oder ein Nachholen lädt neu. Ohne Anmeldung
 * ist die Zahl 0 und die Seite zeigt nur den Satz. Um Mitternacht (lokal)
 * lädt sie neu: eine Frist für morgen wird dann heute, also Jetzt.
 *
 * Etappe A3: Dieselbe Liste speist das Band oben auf jeder Seite, die Zahl im
 * Tab-Titel und den Haken „gelesen“ an Meldungen (`gelesen`).
 *
 * Etappe 3a+: Der Server liefert alle Einträge ohne Zahlen; `auswerten`
 * (anstehend.ts) wählt, ordnet und zählt einmal für alle Seiten.
 *
 * Kopfleiste, Fassung 3: Das Band ist weg; dieselbe Liste speist die Glocke
 * (`glocke`) und die Zähler „wartet“ der Seitenleiste (`wartet`).
 */
import { live } from './live.svelte.ts';
import { beiTageswechsel } from './tageswechsel.ts';
import {
	auswerten,
	behobenUngelesen,
	einstufen,
	glocke,
	ladeAnstehend,
	ladeMeldungen,
	meldungGelesen,
	wartetJeProjekt,
	type AnstehendAnsicht,
	type AnstehendStand,
	type GlockenAnsicht
} from './anstehend.ts';
import { ApiError } from './api/client';
import { mitZugang } from './zugang.ts';
import type { AnstehendEintrag, Meldung, Stufe } from '$vertrag';

export class Anstehend {
	stand = $state<AnstehendStand>('laedt');
	/** Ausgewertet (`auswerten`): eine Frage je Projekt, geordnet, mit Zahlen. */
	liste = $state<AnstehendAnsicht | null>(null);
	fehler = $state<string | null>(null);
	/** Satz, wenn das letzte Umstufen scheiterte. */
	umstufenFehler = $state<string | null>(null);
	/** Anker (`<art>-<id>`) des Eintrags, der gerade umgestuft oder gelesen wird. */
	beschaeftigt = $state<string | null>(null);
	/** Satz, wenn das letzte „gelesen“ scheiterte. */
	gelesenFehler = $state<string | null>(null);
	/**
	 * Kopfleiste, Fassung 3: alle Meldungen aus `/api/meldungen` für den Teil
	 * „behoben“ der Glocke (sie zählen nicht); `null`, solange nicht geholt.
	 */
	meldungenAlle = $state<Meldung[] | null>(null);

	#laeuft: Promise<void> | null = null;
	#nochmal = false;
	#fetch: typeof fetch;

	constructor(fetchFn: typeof fetch = mitZugang((...a) => fetch(...a))) {
		this.#fetch = fetchFn;
	}

	/** Zahl der Stufe Jetzt (für den Reiter), gezählt in `anstehend.ts`. */
	get jetzt(): number {
		return this.liste?.zaehler.jetzt ?? 0;
	}

	/**
	 * Die Glocke (Kopfleiste, Fassung 3): Zahl und Liste aus derselben
	 * Auswertung wie Reiter und Tab-Titel; ohne Liste (ohne Anmeldung) `null`.
	 */
	get glocke(): GlockenAnsicht | null {
		return this.liste === null ? null : glocke(this.liste.eintraege);
	}

	/** Zähler „wartet“ je Projekt für die Seitenleiste; ohne Liste leer. */
	get wartet(): Record<string, number> {
		return this.liste === null ? {} : wartetJeProjekt(this.liste.eintraege);
	}

	/** Behobene, ungelesene Meldungen (nur Liste der Glocke, ohne Zahl). */
	get behoben(): Meldung[] {
		return this.meldungenAlle === null ? [] : behobenUngelesen(this.meldungenAlle);
	}

	/** Holt `/api/meldungen` für den Teil „behoben“ (die Glocke ist offen oder es gab neue). */
	async meldungenLaden(): Promise<void> {
		this.meldungenAlle = this.stand === 'da' ? await ladeMeldungen(this.#fetch) : [];
	}

	#mehr(): boolean {
		return this.#nochmal;
	}

	/** Lädt neu; kommt währenddessen ein Wunsch, folgt genau ein weiterer Lauf. */
	laden(): Promise<void> {
		if (this.#laeuft !== null) {
			this.#nochmal = true;
			return this.#laeuft;
		}
		this.#laeuft = (async () => {
			do {
				this.#nochmal = false;
				const l = await ladeAnstehend(this.#fetch);
				this.stand = l.stand;
				this.liste = l.liste === null ? null : auswerten(l.liste);
				this.fehler = l.fehler;
				// Schon einmal für die Glocke geholt: mit der Liste mitziehen.
				if (this.meldungenAlle !== null) await this.meldungenLaden();
			} while (this.#mehr());
		})().finally(() => {
			this.#laeuft = null;
		});
		return this.#laeuft;
	}

	/** Der Inhaber stuft um (`null`: Vorschlag des Chats); danach neu laden. */
	async umstufen(e: AnstehendEintrag, stufe: Stufe | null): Promise<void> {
		this.beschaeftigt = `${e.art}-${e.id}`;
		this.umstufenFehler = null;
		try {
			await einstufen(e, stufe, this.#fetch);
		} catch (err) {
			this.umstufenFehler =
				err instanceof ApiError ? err.message : 'Die Einstufung ließ sich nicht speichern.';
		} finally {
			this.beschaeftigt = null;
		}
		await this.laden();
	}

	/**
	 * Der Inhaber klickt eine Meldung weg; danach neu laden. Gesperrt bleibt bis
	 * nach dem Neuladen, damit kein zweiter Klick auf eine schon verschwundene
	 * Zeile geht. Gibt zurück, ob es geklappt hat.
	 */
	async gelesen(e: Pick<AnstehendEintrag, 'id'>): Promise<boolean> {
		this.beschaeftigt = `meldung-${e.id}`;
		this.gelesenFehler = null;
		let gut = false;
		try {
			await meldungGelesen(e.id, this.#fetch);
			gut = true;
		} catch (err) {
			this.gelesenFehler =
				err instanceof ApiError ? err.message : 'Die Meldung ließ sich nicht als gelesen ablegen.';
		}
		try {
			await this.laden();
		} finally {
			this.beschaeftigt = null;
		}
		return gut;
	}

	/** Erstes Laden und Live-Kanal; gibt die Abmeldung zurück. */
	start(): () => void {
		void this.laden();
		const offChange = live.onChange((c) => {
			if (c.area === 'anstehend' || c.area === 'aufgaben') void this.laden();
		});
		// Kopfleiste, Fassung 3: Die Glocke folgt auch dem Ereignis `speicher`
		// mit Bereich `meldungen` (etwa eine Meldung wird behoben oder gelesen).
		const offBereich = live.onBereich((b) => {
			if (b === 'meldungen') void this.laden();
		});
		const offNachholen = live.onNachholen(() => {
			void this.laden();
		});
		const offTag = beiTageswechsel(() => {
			void this.laden();
		});
		return () => {
			offChange();
			offBereich();
			offNachholen();
			offTag();
		};
	}
}

export const anstehend = new Anstehend();
