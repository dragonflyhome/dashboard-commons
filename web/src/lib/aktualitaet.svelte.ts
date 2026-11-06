/**
 * Die Aktualitätsprüfung im Blatt der Brotkrumen (Kopfleiste, Fassung 3;
 * Entscheidung immer-aktuell, Schritt 4): „Neu laden“ heißt prüfen,
 * ob die Daten aktuell sind, und zum Seitenanfang scrollen; keine neue
 * Browserseite.
 *
 * Den gemeinsamen Stand liefert der Server (`/api/stand`, angemeldet
 * `/api/stand/voll`): Ereignisnummer und Inhaltsversion. Die Seite merkt sich
 * den Stand beim letzten Abgleich (`basis`) und zählt, was der Live-Kanal
 * seitdem gebracht hat (`empfangen`: dieselben Arten, die der Server zählt:
 * `change`, `vorgang`, `fassung`, `projekt`, `daten`, `inhalt`, `speicher`,
 * `pc-benutzer`, `docs`; frei nur die freien). Beim Klick auf das Blatt:
 *
 * 1. den Stand holen;
 * 2. alles nachziehen (`refreshAll`, früher `invalidateAll`, und die Speicher außerhalb von
 *    `load`: Anstehend, Aufgaben, Passkey-Anträge);
 * 3. vergleichen: Hat der Server mehr Ereignisse geschickt, als hier
 *    ankamen, oder ist die Inhaltsversion weiter als die zuletzt gesehene,
 *    fehlte etwas (`nachgeholt`), sonst war alles `aktuell`.
 *
 * Nachgezogen wird immer (prüfen und nachziehen); der
 * Vergleich sagt nur, ob der Live-Kanal etwas verpasst hatte.
 *
 * Kanalwechsel (Auflage aus der Abnahme): Die Seite zählt je Kanal getrennt
 * und vergleicht den freien nur mit `/api/stand`, den geschützten nur mit
 * `/api/stand/voll`. Steht ein Kanal nach einem Wechsel (`onBereit` in
 * `freikanal.svelte.ts`, also erst wenn sein Strom da ist), holt sie für ihn
 * einen neuen Bezug; der Bezug des anderen verfällt. Gezählt wird ab dem
 * Abschicken der Frage, damit nichts, was unterwegs ankommt, scheinbar fehlt.
 */
import { API } from './api/client';
import { abrufen } from './api/antwortet.svelte.ts';
import { freierKanal } from './freikanal.svelte.ts';
import { live, type InhaltAenderung } from './live.svelte.ts';
import type { ServerStand } from '$vertrag';

/** Ergebnis eines Abgleichs. */
export type Abgleich = 'aktuell' | 'nachgeholt' | 'abgeglichen' | 'ohne-server';

/** Der Satz zum Ergebnis (höfliche Ansage und kurz sichtbar neben dem Blatt). */
export const ABGLEICH_SATZ: Record<Abgleich, string> = {
	aktuell: 'Alles war aktuell.',
	nachgeholt: 'Es fehlte etwas; jetzt ist alles auf dem neuesten Stand.',
	abgeglichen: 'Auf dem neuesten Stand.',
	'ohne-server': 'Der Server antwortet gerade nicht; die Seite zeigt den letzten Stand.'
};

/** Ein Stand mit dem Weg, von dem er kam (frei oder voll zählen verschieden). */
export interface Basis {
	stand: ServerStand;
	voll: boolean;
}

/**
 * Fehlte etwas? `null`: nicht zu sagen (kein früherer Stand, anderer Weg,
 * Server neu gestartet). Sonst die Zahl der Ereignisse, die nicht ankamen,
 * oder 1, wenn nur die Inhaltsversion weiter ist als die zuletzt gesehene.
 */
export function fehlend(
	basis: Basis | null,
	empfangen: number,
	inhaltGesehen: number,
	jetzt: ServerStand,
	voll: boolean
): number | null {
	if (basis?.voll !== voll) return null;
	// Neu gestartet: die Nummer fängt wieder bei null an.
	if (jetzt.ereignis < basis.stand.ereignis) return null;
	const fehlt = jetzt.ereignis - basis.stand.ereignis - empfangen;
	if (fehlt > 0) return fehlt;
	// Die Inhaltsversion steigt auch bei Dateien, die der freie Kanal nicht
	// trägt (data/schluessel.toml): vergleichen nur, wer alles sieht.
	return voll && jetzt.inhalt > Math.max(inhaltGesehen, basis.stand.inhalt) ? 1 : 0;
}

/** Den Stand holen: angemeldet voll, sonst frei (401 auf voll: frei). Wirft nie. */
export async function standHolen(
	voll: boolean,
	fetchFn: typeof fetch = (...a) => fetch(...a)
): Promise<Basis | null> {
	const holen = async (v: boolean): Promise<Response | null> => {
		try {
			return await abrufen(fetchFn, `${API}/stand${v ? '/voll' : ''}`, {
				credentials: 'same-origin',
				headers: { accept: 'application/json' },
				cache: 'no-store'
			});
		} catch {
			return null;
		}
	};
	let res = await holen(voll);
	let weg = voll;
	if (res !== null && voll && (res.status === 401 || res.status === 403)) {
		await res.body?.cancel().catch(() => undefined);
		res = await holen(false);
		weg = false;
	}
	if (!res?.ok) {
		await res?.body?.cancel().catch(() => undefined);
		return null;
	}
	try {
		return { stand: (await res.json()) as ServerStand, voll: weg };
	} catch {
		return null;
	}
}

/** Der Weg zum Stand: frei (`/api/stand`) oder voll (`/api/stand/voll`). */
type Weg = 'frei' | 'voll';

/** Ein gemerkter Stand je Weg: dazu, wie viel bis zum Abschicken der Frage ankam. */
interface Bezug {
	basis: Basis;
	/** `gesehen` dieses Wegs, als die Frage abging. */
	gesehenBei: number;
	/** Nummer der Frage (die später abgeschickte gilt). */
	runde: number;
}

export class Aktualitaet {
	/** Läuft gerade ein Abgleich? (Das Blatt zeigt es, Doppelklicks zählen nicht.) */
	laeuft = $state(false);
	/** Das letzte Ergebnis (kurz sichtbar, dann wieder `null`). */
	ergebnis = $state<Abgleich | null>(null);

	/**
	 * Je Weg der gemerkte Stand. Gezählt wird je Kanal getrennt (was der freie
	 * bringt, vergleicht sich mit `/api/stand`, was der geschützte bringt, mit
	 * `/api/stand/voll`), damit ein Wechsel des Kanals nichts vermischt.
	 */
	#bezug: Record<Weg, Bezug | null> = { frei: null, voll: null };
	/** Je Kanal alles, was je ankam (steigt nur). */
	#gesehen: Record<Weg, number> = { frei: 0, voll: 0 };
	#runde = 0;
	#inhalt = 0;
	#fetch: typeof fetch = (...a) => fetch(...a);
	#aus: (() => void)[] = [];
	#weg: ReturnType<typeof setTimeout> | null = null;

	/** Für Tests: der gemerkte Stand des gültigen Wegs und was seitdem kam. */
	get zustand(): { basis: Basis | null; empfangen: number } {
		const w: Weg = this.#voll() ? 'voll' : 'frei';
		const b = this.#bezug[w];
		return { basis: b?.basis ?? null, empfangen: b === null ? 0 : this.#gesehen[w] - b.gesehenBei };
	}

	/** Ist die Seite angemeldet (der geschützte Kanal gilt)? */
	#voll(): boolean {
		return freierKanal.aktiv === 'geschuetzt';
	}

	/** Mitzählen, was über die Kanäle kommt; den ersten Stand holen. Gibt die Abmeldung zurück. */
	start(fetchFn: typeof fetch = (...a) => fetch(...a)): () => void {
		this.#fetch = fetchFn;
		const voll = (): void => {
			this.#gesehen.voll += 1;
		};
		this.#aus = [
			live.onInhalt((a: InhaltAenderung) => {
				this.#gesehen.voll += 1;
				this.#inhalt = Math.max(this.#inhalt, a.version);
			}),
			freierKanal.onInhalt(() => {
				this.#gesehen.frei += 1;
			}),
			freierKanal.onFassung((_f, kanal) => {
				this.#gesehen[kanal === 'geschuetzt' ? 'voll' : 'frei'] += 1;
			}),
			live.onChange(voll),
			live.onVorgang(voll),
			live.onBereich(voll),
			// Ein Kanal steht nach Start oder Wechsel: neuer Bezug für ihn; der des
			// anderen gilt nicht mehr (der war zu, was er verpasst hat, fehlte nicht).
			freierKanal.onBereit((kanal) => {
				const v = kanal === 'geschuetzt';
				this.#bezug[v ? 'frei' : 'voll'] = null;
				void this.basisHolen(v);
			})
		];
		void this.basisHolen();
		return () => {
			for (const a of this.#aus) a();
			this.#aus = [];
			if (this.#weg !== null) clearTimeout(this.#weg);
			this.#weg = null;
		};
	}

	/**
	 * Den Stand holen und als Bezug seines Wegs merken. Gezählt wird ab dem
	 * Abschicken der Frage: Was unterwegs ankommt, zählt mit (sonst fehlte
	 * es beim nächsten Abgleich scheinbar). Gibt den Stand zurück und, wie
	 * viel gegenüber dem vorigen Bezug fehlte (`null`: nicht zu sagen).
	 */
	async #holen(voll: boolean): Promise<{ jetzt: Basis | null; fehlt: number | null }> {
		const marke = { ...this.#gesehen };
		const runde = ++this.#runde;
		const jetzt = await standHolen(voll, this.#fetch);
		if (jetzt === null) return { jetzt, fehlt: null };
		const w: Weg = jetzt.voll ? 'voll' : 'frei';
		const vorher = this.#bezug[w];
		const fehlt =
			vorher === null
				? null
				: fehlend(
						vorher.basis,
						this.#gesehen[w] - vorher.gesehenBei,
						this.#inhalt,
						jetzt.stand,
						jetzt.voll
					);
		if (vorher === null || vorher.runde < runde) {
			this.#bezug[w] = { basis: jetzt, gesehenBei: marke[w], runde };
		}
		this.#inhalt = Math.max(this.#inhalt, jetzt.stand.inhalt);
		return { jetzt, fehlt };
	}

	/** Neuer Bezug (beim Start und wenn ein Kanal nach einem Wechsel steht). */
	async basisHolen(voll: boolean = this.#voll()): Promise<void> {
		await this.#holen(voll);
	}

	/**
	 * Der Klick auf das Blatt: Stand holen, alles nachziehen (`nachziehen`),
	 * vergleichen. Gibt das Ergebnis zurück; `ergebnis` steht 6 s lang.
	 */
	async pruefen(nachziehen: () => Promise<unknown>): Promise<Abgleich> {
		if (this.laeuft) return this.ergebnis ?? 'abgeglichen';
		this.laeuft = true;
		try {
			const { jetzt, fehlt } = await this.#holen(this.#voll());
			await nachziehen();
			let e: Abgleich;
			if (jetzt === null) e = 'ohne-server';
			else if (fehlt === null) e = 'abgeglichen';
			else e = fehlt > 0 ? 'nachgeholt' : 'aktuell';
			this.#zeigen(e);
			return e;
		} finally {
			this.laeuft = false;
		}
	}

	#zeigen(e: Abgleich): void {
		this.ergebnis = e;
		if (this.#weg !== null) clearTimeout(this.#weg);
		this.#weg = setTimeout(() => {
			this.#weg = null;
			this.ergebnis = null;
		}, 6000);
	}

	/** Für Tests. */
	vergessen(): void {
		for (const a of this.#aus) a();
		this.#aus = [];
		this.#bezug = { frei: null, voll: null };
		this.#gesehen = { frei: 0, voll: 0 };
		this.#runde = 0;
		this.#inhalt = 0;
		this.ergebnis = null;
		this.laeuft = false;
	}
}

/** Eine für die ganze Seite. */
export const aktualitaet = new Aktualitaet();
