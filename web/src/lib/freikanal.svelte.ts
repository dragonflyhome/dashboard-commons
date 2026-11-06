/**
 * Der freie Kanal `/api/events/frei` und die Wahl des einen Kanals je Seite
 * (Kopfleiste, Fassung 2, mit Nachbesserung aus der Abnahme).
 *
 * Der freie Kanal steht auch ohne Anmeldung und trägt nur das Lebenszeichen,
 * den Anmeldestand (`anmeldung`, ein Wort vom Server) und Ereignisse zu
 * Daten, die ohnehin frei sind (`projekt`, `daten` für pc und schwerpunkte, `inhalt`, `pc`,
 * `fassung`, `nachholen`). Vorgänge, Antworten und Schlüssel bleiben im
 * geschützten Kanal (`live.svelte.ts`).
 *
 * Nur EIN Kanal je Seite: Edge erlaubt über HTTP/1.1 sechs Verbindungen je
 * Host über alle Tabs; zwei Kanäle je Seite ließen ab dem vierten Tab
 * Anfragen hängen. Angemeldet (die Frage beim Start in `zugang.ts` gab 200)
 * ist nur der geschützte offen, sonst nur der freie. Der Wechsel:
 *
 * - Anmelden (frei → geschützt): Der geschützte öffnet; erst wenn er sein
 *   erstes Ereignis (`anmeldung`) hat, schließt der freie. Scheitert er
 *   (401), bleibt der freie.
 * - Abmelden, Widerruf, Ablauf (geschützt → frei): Der Server schickt auf
 *   dem geschützten `anmeldung` mit `nicht_angemeldet` und beendet den Strom,
 *   oder der Neuaufbau bekommt 401. Dann öffnet der freie; ein noch offener
 *   geschützter schließt, sobald der freie sein erstes Ereignis hat. Der
 *   Anmeldestand der Seite (`zugang`) wird neu erfragt. Während des Wechsels
 *   bleibt der Zustand „live“ (nie „nicht gefunden“), das Schloss geht
 *   sofort zu.
 *
 * Aus dem gerade gültigen Kanal kommt der Verbindungszustand: `sucht` (erster
 * Versuch, höchstens `SUCHEN_MS`), `nie`, `offen`, `weg` (stand und ist weg;
 * die Seite versucht es selbst neu). Gibt der Browser den freien Kanal auf
 * (`CLOSED`), versucht er es nach 2 s, 5 s, 15 s, dann alle 30 s.
 *
 * Den Anmeldestand nennt der Server; der freie Kanal sieht die
 * Windows-Anmeldung aber nur auf einer schon angemeldeten Verbindung. Hat
 * `zugang` eine 401 mit Art `passkey` (fremdes Windows-Konto), fragt der
 * Kanal einmal `/api/passkey/antrag` nach dem Wort für dieses Konto. Was
 * daraus wird, rechnet `verbindung.ts`.
 *
 * Für die Aktualitätsprüfung (`aktualitaet.svelte.ts`) meldet der Kanal,
 * wann ein Kanal nach dem Start oder einem Wechsel wirklich steht
 * (`onBereit`, mit seinem Namen), und bei jeder Fassung, aus welchem Kanal
 * sie kam: Server und Seite zählen je Kanal verschieden.
 */
import { SvelteSet } from 'svelte/reactivity';
import { API } from './api/client';
import {
	alsAenderung,
	live,
	wartezeit,
	type InhaltListener,
	type KanalMeldung
} from './live.svelte.ts';
import {
	anmeldewort,
	liveZustand,
	SUCHEN_MS,
	type KanalStand,
	type LiveZustand
} from './verbindung.ts';
import { bauLage } from './bau.svelte.ts';
import { zugang } from './zugang.ts';
import type { AnmeldungEvent, Anmeldewort, EigeneAntraege, FassungEvent, PcTick } from '$vertrag';

/** Die Adresse des freien Kanals. */
export const FREIER_KANAL = `${API}/events/frei`;

/** Welcher Kanal gerade gilt. */
export type Aktiv = Kanal | null;

/** Einer der zwei Kanäle. */
export type Kanal = 'frei' | 'geschuetzt';

/** Eine neue Fassung, mit dem Kanal, aus dem sie kam. */
export type KanalFassungListener = (fassung: FassungEvent, kanal: Kanal) => void;

/** Ein Kanal steht nach dem Start oder einem Wechsel (sein erstes Ereignis ist da). */
export type BereitListener = (kanal: Kanal) => void;

/** JSON aus dem Kanal; die Form garantiert der Rust-Server (siehe $vertrag). */
function lesen(data: string): unknown {
	try {
		return JSON.parse(data);
	} catch {
		return null;
	}
}

const WOERTER: readonly Anmeldewort[] = ['angemeldet', 'nicht_angemeldet', 'nicht_registriert'];

function istWort(w: unknown): w is Anmeldewort {
	return typeof w === 'string' && (WOERTER as readonly string[]).includes(w);
}

export class FreierKanal {
	/** Der Zustand der Verbindung (siehe oben). */
	kanal = $state<KanalStand>('sucht');
	/** Das Wort des gültigen Kanals zum Anmeldestand, sobald es da ist. */
	wort = $state<Anmeldewort | null>(null);
	/** Das Wort von `/api/passkey/antrag` (nur bei einem fremden Windows-Konto). */
	kontoWort = $state<Anmeldewort | null>(null);
	/** Die letzten PC-Werte vom freien Kanal (frei wie `/api/pc`). */
	pc = $state<PcTick | null>(null);
	/** Welcher Kanal gilt: der freie, der geschützte oder (vor dem Start) keiner. */
	aktiv = $state<Aktiv>(null);

	#source: EventSource | null = null;
	#versuch = 0;
	#timer: ReturnType<typeof setTimeout> | null = null;
	#inhaltListeners = new SvelteSet<InhaltListener>();
	#fassungListeners = new SvelteSet<KanalFassungListener>();
	#bereitListeners = new SvelteSet<BereitListener>();
	/** Nach Start oder Wechsel: dieser Kanal meldet sein erstes Ereignis als „bereit“. */
	#bereitMelden: Kanal | null = null;
	#kontoGefragt = false;
	#suchen: ReturnType<typeof setTimeout> | null = null;
	#fetch: typeof fetch = (...a) => fetch(...a);
	#offLive: (() => void) | null = null;
	/** Anmelden läuft: der geschützte öffnet, der freie wartet auf dessen erstes Ereignis. */
	#zumGeschuetzten = false;
	/** Abmelden läuft: der freie öffnet, der geschützte schließt bei dessen erstem Ereignis. */
	#zumFreien = false;

	/** Angemeldet: der geschützte Kanal gilt und ist offen. */
	get geschuetztOk(): boolean {
		return this.aktiv === 'geschuetzt' && live.connected;
	}

	/** Der Zustand des Live-Punkts (fünf Werte; `null`, solange der erste Versuch läuft). */
	get zustand(): LiveZustand | null {
		return liveZustand(this.kanal, anmeldewort(this.geschuetztOk, this.wort, this.kontoWort));
	}

	/** Startet den einen Kanal: angemeldet den geschützten, sonst den freien. */
	start(fetchFn: typeof fetch = (...a) => fetch(...a)): void {
		if (this.aktiv !== null || typeof EventSource === 'undefined') return;
		this.#fetch = fetchFn;
		this.#offLive ??= live.onKanal((m) => {
			this.#vomGeschuetzten(m);
		});
		this.#suchen ??= setTimeout(() => {
			this.#suchen = null;
			if (this.kanal === 'sucht') this.kanal = 'nie';
		}, SUCHEN_MS);
		if (zugang.stand === 'da') {
			this.aktiv = 'geschuetzt';
			this.#bereitMelden = 'geschuetzt';
			live.start();
		} else {
			this.aktiv = 'frei';
			this.#bereitMelden = 'frei';
			this.#freiOeffnen();
		}
	}

	/** Der Kanal `k` steht (einmal nach Start oder Wechsel). */
	#bereit(k: Kanal): void {
		if (this.#bereitMelden !== k) return;
		this.#bereitMelden = null;
		for (const l of this.#bereitListeners) l(k);
	}

	stop(): void {
		this.#timerWeg();
		if (this.#suchen !== null) clearTimeout(this.#suchen);
		this.#suchen = null;
		this.#freiSchliessen();
		live.stop();
		this.#offLive?.();
		this.#offLive = null;
		this.aktiv = null;
		this.#zumGeschuetzten = false;
		this.#zumFreien = false;
		this.#bereitMelden = null;
		this.#versuch = 0;
	}

	/**
	 * Abgemeldet (Knopf „Abmelden“ auf /anmelden): Zustand zurücksetzen und
	 * zum freien Kanal wechseln. Dasselbe geschieht von selbst, wenn der
	 * Server die Sitzung beendet (Widerruf, Ablauf).
	 */
	abgemeldet(): void {
		if (this.aktiv === 'geschuetzt') this.#zuFrei();
	}

	/** Für Tests: ist gerade eine EventSource des freien Kanals offen? */
	get freiOffen(): boolean {
		return this.#source !== null;
	}

	// ----- Wechsel -----

	/** Anmelden erkannt: den geschützten öffnen, der freie bleibt bis zu dessen erstem Ereignis. */
	#zuGeschuetzt(): void {
		if (this.aktiv !== 'frei' || this.#zumGeschuetzten) return;
		this.#zumGeschuetzten = true;
		live.start();
	}

	/** Abmelden erkannt: Zustand zurück, den freien öffnen, der geschützte schließt danach. */
	#zuFrei(): void {
		this.aktiv = 'frei';
		this.#bereitMelden = 'frei';
		this.#zumGeschuetzten = false;
		this.#zumFreien = true;
		this.wort = null;
		this.kontoWort = null;
		this.#kontoGefragt = false;
		// Bau-Tempo, Zweig C: Ohne Anmeldung kein Schrittzähler, auch nicht der letzte.
		bauLage.fortschritt = null;
		// Den Anmeldestand der Seite neu erfragen (geschützte Abfragen wieder vom Server).
		zugang.vergessen();
		void zugang.pruefen(this.#fetch).then((stand) => {
			if (stand === 'da' && this.aktiv === 'frei') {
				// Es gilt noch eine Anmeldung (etwa Windows nach dem Abmelden des Passkeys):
				// zurück zum geschützten Kanal; der freie schließt bei dessen erstem Ereignis.
				if (this.#zumFreien) {
					this.#zumFreien = false;
					live.stop();
				}
				this.#zuGeschuetzt();
			} else {
				void this.#kontoFragen();
			}
		});
		// Kein „nicht gefunden“ während des Wechsels: es stand eine Verbindung.
		this.kanal = 'offen';
		this.#freiOeffnen();
		// Ist der geschützte schon zu (Strom zu Ende, 401), bleibt nichts zu schließen.
		if (!live.connected) {
			live.stop();
			this.#zumFreien = false;
		}
	}

	/** Was der geschützte Kanal meldet. */
	#vomGeschuetzten(m: KanalMeldung): void {
		if (m.art === 'fassung') {
			if (this.aktiv === 'geschuetzt') {
				for (const l of this.#fassungListeners) l(m.fassung, 'geschuetzt');
			}
			return;
		}
		if (this.#zumGeschuetzten) {
			// Anmelden: Der geschützte hat sein erstes Ereignis, jetzt erst den freien schließen.
			if (m.art === 'anmeldung' && m.wort === 'angemeldet') {
				this.#zumGeschuetzten = false;
				this.aktiv = 'geschuetzt';
				this.wort = m.wort;
				this.kanal = 'offen';
				this.#freiSchliessen();
				this.#bereitMelden = 'geschuetzt';
				this.#bereit('geschuetzt');
			} else if (m.art === 'aufgegeben') {
				// 401: es bleibt beim freien.
				this.#zumGeschuetzten = false;
			}
			return;
		}
		if (this.aktiv !== 'geschuetzt') return;
		switch (m.art) {
			case 'offen':
				this.kanal = 'offen';
				this.#versuch = 0;
				break;
			case 'fehler':
				if (this.kanal === 'offen') this.kanal = 'weg';
				else if (this.kanal === 'sucht') this.kanal = 'nie';
				break;
			case 'anmeldung':
				if (m.wort === 'angemeldet') {
					this.wort = m.wort;
					this.kanal = 'offen';
					this.#bereit('geschuetzt');
				} else {
					// Der Server hat die Sitzung beendet (Widerruf, Abmelden).
					this.#zuFrei();
				}
				break;
			case 'aufgegeben':
				// Neuaufbau mit 401 (Ausweis abgelaufen): zum freien Kanal.
				this.#zuFrei();
				break;
		}
	}

	// ----- der freie Kanal -----

	#freiOeffnen(): void {
		if (this.#source !== null) return;
		this.#timerWeg();
		const source = new EventSource(FREIER_KANAL);
		this.#source = source;
		const erstes = () => {
			if (this.#source !== source) return;
			this.kanal = 'offen';
			this.#versuch = 0;
			if (this.#zumFreien) {
				// Abmelden: Der freie steht, jetzt erst den geschützten schließen.
				this.#zumFreien = false;
				live.stop();
			}
			if (this.aktiv === 'frei') this.#bereit('frei');
		};
		source.onopen = erstes;
		source.onerror = () => {
			if (this.#source !== source) return;
			// Nur wer schon verbunden war, ist „unterbrochen“; sonst bleibt es bei „nie“.
			if (this.kanal === 'offen') this.kanal = 'weg';
			else if (this.kanal === 'sucht') this.kanal = 'nie';
			if (source.readyState === EventSource.CLOSED) this.#spaeter(source);
		};
		source.addEventListener('anmeldung', (ev: MessageEvent<string>) => {
			if (this.#source !== source) return;
			const a = lesen(ev.data) as AnmeldungEvent | null;
			if (istWort(a?.stand)) this.wort = a.stand;
			erstes();
			// Beim Start nicht zu erfahren (Server noch nicht da)? Jetzt fragen; angemeldet: wechseln.
			if (zugang.stand === 'unbekannt') {
				void zugang.pruefen(this.#fetch).then((s) => {
					if (s === 'da') this.#zuGeschuetzt();
					else void this.#kontoFragen();
				});
			}
		});
		source.addEventListener('pc', (ev: MessageEvent<string>) => {
			const t = lesen(ev.data) as PcTick | null;
			if (t !== null) this.pc = t;
		});
		for (const name of ['projekt', 'daten', 'inhalt']) {
			source.addEventListener(name, (ev: MessageEvent<string>) => {
				const a = alsAenderung(name, lesen(ev.data));
				if (a === null) return;
				for (const l of this.#inhaltListeners) l(a);
			});
		}
		source.addEventListener('fassung', (ev: MessageEvent<string>) => {
			const f = lesen(ev.data) as FassungEvent | null;
			if (f === null) return;
			for (const l of this.#fassungListeners) l(f, 'frei');
		});
		void this.#kontoFragen();
	}

	#freiSchliessen(): void {
		this.#timerWeg();
		this.#source?.close();
		this.#source = null;
	}

	/** Der Browser hat den freien Kanal aufgegeben: später selbst neu versuchen. */
	#spaeter(source: EventSource): void {
		if (this.#source !== source) return;
		source.close();
		this.#source = null;
		const ms = wartezeit(this.#versuch);
		this.#versuch += 1;
		this.#timer = setTimeout(() => {
			this.#timer = null;
			if (this.aktiv === 'frei') this.#freiOeffnen();
		}, ms);
	}

	#timerWeg(): void {
		if (this.#timer !== null) clearTimeout(this.#timer);
		this.#timer = null;
	}

	/**
	 * Fremdes Windows-Konto erkannt (401 mit Art `passkey`): einmal nach dem
	 * Wort für dieses Konto fragen. Still bei Fehlern.
	 */
	async #kontoFragen(): Promise<void> {
		if (this.#kontoGefragt || zugang.art !== 'passkey') return;
		this.#kontoGefragt = true;
		try {
			const res = await this.#fetch(`${API}/passkey/antrag`, {
				credentials: 'same-origin',
				headers: { accept: 'application/json' }
			});
			if (!res.ok) {
				await res.body?.cancel().catch(() => undefined);
				return;
			}
			const e = (await res.json()) as EigeneAntraege;
			if (istWort(e.anmeldung)) this.kontoWort = e.anmeldung;
		} catch {
			// Kein Wort: es bleibt beim Wort des Kanals.
		}
	}

	/** Meldet sich bei jeder Änderung am freien Inhalt; gibt die Abmeldung zurück. */
	onInhalt(listener: InhaltListener): () => void {
		this.#inhaltListeners.add(listener);
		return () => this.#inhaltListeners.delete(listener);
	}

	/** Meldet sich bei jeder neuen Fassung der Oberfläche (beide Kanäle); gibt die Abmeldung zurück. */
	onFassung(listener: KanalFassungListener): () => void {
		this.#fassungListeners.add(listener);
		return () => this.#fassungListeners.delete(listener);
	}

	/**
	 * Meldet sich, sobald ein Kanal nach dem Start oder einem Wechsel steht
	 * (sein erstes Ereignis ist da; nicht bei einem Neuaufbau desselben
	 * Kanals). Gibt die Abmeldung zurück.
	 */
	onBereit(listener: BereitListener): () => void {
		this.#bereitListeners.add(listener);
		return () => this.#bereitListeners.delete(listener);
	}

	/** Für Tests. */
	vergessen(): void {
		this.stop();
		this.kanal = 'sucht';
		this.wort = null;
		this.kontoWort = null;
		this.pc = null;
		this.#kontoGefragt = false;
	}
}

/** Der eine Kanal-Wähler für die ganze Seite. */
export const freierKanal = new FreierKanal();
