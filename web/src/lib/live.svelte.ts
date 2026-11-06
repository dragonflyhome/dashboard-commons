/**
 * Live-Kanal zum Rust-Server (Server-Sent Events, `/api/events`).
 * `change`: eine Datei in projects/ oder data/ hat sich geändert.
 * `pc`: PC-Werte, alle 5 Sekunden.
 * `vorgang`: ein neues Ereignis zu einem Vorgang (Kennung, Slug, Art, Zustand, Nummer).
 *
 * Etappe 2d, genaue Ereignisse aus dem Inhalt im Arbeitsspeicher des Servers:
 * `projekt` (slug, version): die Daten dieses Projekts sind neu (oder sein
 * Git-Stand). `daten` (name, version): `data/<name>.toml` ist neu. `inhalt`
 * (version, projekte, daten): viele auf einmal, ein Ereignis statt vieler.
 * Alle drei kommen gebündelt als `InhaltAenderung` bei `onInhalt` an; die
 * Seite lädt damit nur, was betroffen ist (`betroffen`), statt alles.
 *
 * `bau` (Etappe 4c, nur hier im geschützten Kanal): der Baumeister baut eine
 * Seite (slug, zustand `laeuft`, `gruen` oder `rot`, Satz); bei `onBau`.
 * `bau-fortschritt` (Bau-Tempo, Zweig C, ebenso nur hier): Schritt und Zähler
 * des laufenden Baus, nur Zahlen; bei `onBauFortschritt`.
 *
 * `nachholen`: der Server hat Ereignisse verworfen, weil die Seite zu langsam
 * las (Daten `{ "grund": "ueberlauf" }`).
 *
 * Nachholen: Fehlen Vorgangs-Ereignisse (Lücke in `nr`), war die Verbindung
 * weg und ist wieder da, oder schickt der Server `nachholen`, meldet der Kanal
 * „nachholen“; die Seite lädt dann alles neu.
 *
 * Anmeldung (7b): Der Live-Kanal ist nur für das Windows-Konto des Inhabers. Ohne
 * Anmeldung (Dashboard über 127.0.0.1 statt localhost geöffnet) antwortet
 * `/api/events` mit 401; EventSource meldet dann nur `error`, ohne Code, und
 * gibt auf (`readyState` CLOSED). Bei jedem solchen Fehler fragt der Kanal
 * darum `/api/vorgaenge/uebersicht` (ebenfalls geschützt): 401 heißt „ohne
 * Anmeldung“, 403 mit Art `fremdes-konto` „fremdes Konto“; in beiden Fällen
 * bleibt er zu. Sonst (Netzfehler, Server startet neu, 503 …) versucht er es
 * selbst wieder, nach 2 s, 5 s, 15 s, dann alle 30 s. Steht `readyState` auf
 * CONNECTING, verbindet der Browser selbst neu; dann tut der Kanal nichts.
 *
 * Rückfall (3a+): Öffnet der Kanal, ist die Verbindung angemeldet. Hatte
 * sich die Seite den freien Weg gemerkt (`rueckfall.ts`, nach einem kurzen
 * 401 auf `…/voll`), gilt das ab jetzt nicht mehr; der Kanal meldet dann
 * „nachholen“ mit Grund `angemeldet`, und die Seite lädt über `/voll` neu.
 *
 * Zugang (3b): Weiß die Seite schon, dass keine Anmeldung möglich ist
 * (`zugang.ts`, eine Frage beim Start), öffnet der Kanal gar nicht erst; das
 * spart das 401 des Kanals und die Nachfrage danach.
 *
 * Kopfleiste, Fassung 2 (Nachbesserung): Je Seite ist nur ein Kanal offen.
 * Angemeldet dieser, sonst der freie (`freikanal.svelte.ts` entscheidet und
 * startet ihn). Der Server schickt hier als Erstes `anmeldung` (dasselbe
 * Ereignis wie auf dem freien Kanal); endet eine Sitzung mit Passkey
 * (Widerruf, Abmelden), kommt `anmeldung` mit `nicht_angemeldet` und der
 * Strom endet. Was hier geschieht, meldet `onKanal` dem freien Kanal.
 *
 * Live-Lücken (Entscheidung immer-aktuell, Punkt 2): Drei Arten
 * mit Bereich, nur hier im geschützten Kanal: `speicher` (der Datenspeicher
 * hat geschrieben: Aufgaben, Arbeiten, Einstufung, Meldungen, Antworten,
 * Vorgänge, Passkeys, Sicherung, PC-Messung, Windows-Ereignisse, Tabellen der
 * Projekte), `pc-benutzer` (Ordnergrößen, Systemprogramme, Tresor, Bote da
 * oder weg) und `docs` (Entscheidungen, Zielbild). Sie kommen bei `onBereich`
 * an; `bereichBetroffen` sagt, welche `depends`-Schlüssel neu laden. Ein
 * `projekt` mit `assets: true` meldet geänderte Dateien in `assets/`
 * (`InhaltAenderung.assets`, siehe `assets.svelte.ts`).
 */
import { SvelteSet } from 'svelte/reactivity';
import { API } from './api/client';
import { rueckfall } from './rueckfall.ts';
import { zugang } from './zugang.ts';
import type {
	AnmeldungEvent,
	BauEvent,
	BauFortschrittEvent,
	Anmeldewort,
	BereichEvent,
	LiveBereich,
	FassungEvent,
	ChangeEvent,
	DatenEvent,
	InhaltEvent,
	NachholEvent,
	PcTick,
	ProjektEvent,
	VorgangEvent
} from '$vertrag';

export type ChangeListener = (change: ChangeEvent) => void;
export type VorgangListener = (vorgang: VorgangEvent) => void;
/** Etappe 4c: ein Schritt des Baumeisters für eine Seite (Ereignis `bau`). */
export type BauListener = (bau: BauEvent) => void;
/** Zweig C: Schritt und Zähler des laufenden Baus (Ereignis `bau-fortschritt`). */
export type BauFortschrittListener = (f: BauFortschrittEvent) => void;
/**
 * Grund fürs Nachholen: Lücke in den Nummern, neu verbunden, Überlauf beim
 * Server oder angemeldet (ein gemerkter Rückfall auf den freien Weg ist weg).
 */
export type NachholGrund = 'luecke' | 'wiederverbunden' | 'angemeldet' | NachholEvent['grund'];
export type NachholListener = (grund: NachholGrund) => void;

/**
 * Eine Änderung am Inhalt (Etappe 2d), aus `projekt`, `daten` oder `inhalt`.
 * `sammel`: ein Sammelereignis; dann gilt sie für alles, was die Seite zeigt.
 */
export interface InhaltAenderung {
	projekte: string[];
	daten: string[];
	version: number;
	sammel: boolean;
	/** Live-Lücken, Punkt 5: Projekte, deren Dateien in `assets/` neu sind (nur wenn welche). */
	assets?: string[];
	/**
	 * Bei `assets`: die Namen der geänderten Dateien (nur auf dem geschützten
	 * Kanal). Fehlt das Feld, gilt die Änderung für alle Dateien des Projekts.
	 */
	assetDateien?: string[];
}
export type InhaltListener = (aenderung: InhaltAenderung) => void;
/** Eine neue Fassung der Oberfläche ist da (Ereignis `fassung`, Etappe 4b). */
export type FassungListener = (fassung: FassungEvent) => void;

/** Macht aus einem der drei Ereignisse eine `InhaltAenderung`; Unlesbares wird `null`. */
export function alsAenderung(name: string, daten: unknown): InhaltAenderung | null {
	if (daten === null || typeof daten !== 'object') return null;
	if (name === 'projekt') {
		const p = daten as ProjektEvent;
		const a: InhaltAenderung = { projekte: [p.slug], daten: [], version: p.version, sammel: false };
		if (p.assets === true) {
			a.assets = [p.slug];
			if (Array.isArray(p.dateien)) a.assetDateien = p.dateien;
		}
		return a;
	}
	if (name === 'daten') {
		const d = daten as DatenEvent;
		return { projekte: [], daten: [d.name], version: d.version, sammel: false };
	}
	if (name === 'inhalt') {
		const i = daten as InhaltEvent;
		return { projekte: i.projekte, daten: i.daten, version: i.version, sammel: true };
	}
	return null;
}

/** Betrifft die Änderung dieses Projekt? */
export function betrifftProjekt(a: InhaltAenderung, slug: string): boolean {
	return a.sammel || a.projekte.includes(slug);
}

/** Welche Datei aus `data/` welche Abhängigkeit (`depends`) neu lädt. */
const DATEN_ABHAENGIGKEIT: Record<string, string> = {
	pc: 'api:pc',
	schluessel: 'api:schluessel',
	// Kopfleiste, Fassung 3: Die Seitenleiste gruppiert die Projekte nach Schwerpunkt.
	schwerpunkte: 'api:projects',
	// Entscheidung projektgruppe: die Gruppen stehen in der Projektliste.
	gruppen: 'api:projects'
};

/**
 * Für `invalidate`: welche Ladefunktionen eine Änderung neu laden muss. Ein
 * Projekt: die Projektliste (`api:projects`), die PC-Werte mit dem Backup je
 * Projekt (`api:pc`) und seine Seiten
 * (`api:project:<slug>`, `api:projekt:<slug>`). Ein Sammelereignis: alle
 * Projektseiten. Eine Datei aus `data/`: die Seite, die sie zeigt.
 */
export function betroffen(a: InhaltAenderung): (url: URL) => boolean {
	const schluessel: string[] = [];
	// Die PC-Seite zeigt das Backup je Projekt (aus /api/pc), darum auch api:pc.
	if (a.projekte.length > 0 || a.sammel) schluessel.push('api:projects', 'api:pc');
	for (const s of a.projekte) {
		schluessel.push(`api:project:${s}`, `api:projekt:${s}`);
	}
	for (const d of a.daten) {
		const k = DATEN_ABHAENGIGKEIT[d];
		if (k !== undefined) schluessel.push(k);
	}
	return (url) => {
		const h = url.href;
		if (schluessel.includes(h)) return true;
		return a.sammel && (h.startsWith('api:project:') || h.startsWith('api:projekt:'));
	};
}

/** Ein Ereignis mit Bereich (`speicher`, `pc-benutzer`, `docs`). */
export type BereichListener = (bereich: LiveBereich) => void;

/** Die Namen der Ereignisse mit Bereich. */
export const BEREICH_EREIGNISSE = ['speicher', 'pc-benutzer', 'docs'] as const;

/**
 * Welche `depends`-Schlüssel ein Bereich neu lädt. `api:daten` ist /daten
 * (Tabellen, Sicherungen, PC-Verlauf), `api:verlauf` der Verlauf auf / und
 * /pc (Windows-Ereignisse, Meldungen, Sicherungen, Vorgänge), `api:pc:benutzer`
 * die Benutzer-Daten auf / und /pc (Cockpit-Kacheln, Ordner, Programme,
 * Schlüsselstand), `api:schluessel` die Seite /schluessel. Bausteine, die
 * selbst holen (Speicher aufgaben und anstehend, PasskeyVerwaltung,
 * SystemMeldungen), hören selbst auf `change` bzw. `onBereich`.
 */
export const BEREICH_ABHAENGIGKEIT: Record<LiveBereich, readonly string[]> = {
	aufgaben: ['api:daten'],
	arbeiten: ['api:daten'],
	einstufung: ['api:daten'],
	meldungen: ['api:daten', 'api:verlauf'],
	antworten: ['api:daten'],
	// Die Seite /vorgaenge lädt schon beim Ereignis `vorgang` (`vorgangBetroffen`);
	// `api:vorgaenge` hier zur Sicherung, falls ein Schreibweg `vorgang` vergisst.
	vorgaenge: ['api:daten', 'api:verlauf', 'api:vorgaenge'],
	passkeys: ['api:daten'],
	sicherung: ['api:daten', 'api:verlauf'],
	'pc-messung': ['api:daten'],
	'windows-ereignisse': ['api:daten', 'api:verlauf'],
	// Ohne Slug: keine Projektseite zeigt eine Modul-Tabelle (siehe
	// `projektseiten_zeigen_keine_modul_tabellen`), nur /daten zählt sie.
	'projekt-tabellen': ['api:daten'],
	inventar: ['api:pc:benutzer'],
	systemprogramme: ['api:pc:benutzer'],
	tresor: ['api:pc:benutzer', 'api:schluessel'],
	bote: ['api:pc:benutzer', 'api:schluessel'],
	entscheidungen: ['api:entscheidungen'],
	zielbild: ['api:zielbild']
};

/** Für `invalidate`: die Ladefunktionen, die ein Bereich betrifft. */
export function bereichBetroffen(bereich: LiveBereich): (url: URL) => boolean {
	const schluessel = BEREICH_ABHAENGIGKEIT[bereich];
	return (url) => schluessel.includes(url.href);
}

/**
 * Für `invalidate` beim Ereignis `vorgang`: nur die Übersicht der Vorgänge.
 * Was ein Vorgang sonst ändert, kommt als `speicher` mit Bereich `vorgaenge`
 * (/daten, Verlauf) oder hört selbst (Unter-Dashboard, Passkeys).
 */
export function vorgangBetroffen(): (url: URL) => boolean {
	return (url) => url.href === 'api:vorgaenge';
}

function istBereich(b: unknown): b is LiveBereich {
	return typeof b === 'string' && Object.hasOwn(BEREICH_ABHAENGIGKEIT, b);
}

/** JSON aus dem Kanal; die Form garantiert der Rust-Server (siehe $vertrag). */
function parse(data: string): unknown {
	try {
		return JSON.parse(data);
	} catch {
		return null;
	}
}

/**
 * Fehlt zwischen der zuletzt gesehenen und der neuen Nummer etwas? Ohne
 * vorherige Nummer (gerade verbunden) gibt es keine Lücke. Eine kleinere
 * Nummer (Server neu gestartet) zählt auch nicht als Lücke.
 */
export function luecke(letzte: number | null, neue: number): boolean {
	return letzte !== null && neue > letzte + 1;
}

/**
 * Was sagt der Server zur Anmeldung? `fehlt`: 401. `fremd`: 403 mit Art
 * `fremdes-konto`. `sonst`: alles andere, auch ein Netzfehler (Server startet
 * neu); dann lohnt ein neuer Versuch.
 */
export type Anmeldestand = 'fehlt' | 'fremd' | 'sonst';

/** Fragt einen geschützten Weg, wie es um die Windows-Anmeldung steht. */
export async function anmeldungPruefen(fetchFn: typeof fetch = fetch): Promise<Anmeldestand> {
	try {
		const res = await fetchFn(`${API}/vorgaenge/uebersicht`, {
			headers: { accept: 'application/json' }
		});
		if (res.status === 401) return 'fehlt';
		if (res.status === 403) {
			const body = (await res.json().catch(() => null)) as { art?: unknown } | null;
			if (body?.art === 'fremdes-konto') return 'fremd';
		}
		return 'sonst';
	} catch {
		return 'sonst';
	}
}

/** Wartezeiten vor einem neuen Versuch, in ms; danach bleibt es beim letzten Wert. */
export const NEU_VERSUCHEN_MS = [2000, 5000, 15000, 30000] as const;

/** Wartezeit vor dem Versuch `versuch` (ab 0). */
export function wartezeit(versuch: number): number {
	return NEU_VERSUCHEN_MS[Math.min(versuch, NEU_VERSUCHEN_MS.length - 1)] ?? 30000;
}

/** Was der geschützte Kanal dem freien meldet (Wechsel zwischen den Kanälen). */
export type KanalMeldung =
	| { art: 'offen' }
	| { art: 'fehler' }
	| { art: 'aufgegeben'; stand: 'fehlt' | 'fremd' }
	| { art: 'anmeldung'; wort: Anmeldewort }
	| { art: 'fassung'; fassung: FassungEvent };
export type KanalListener = (m: KanalMeldung) => void;

const WOERTER: readonly string[] = ['angemeldet', 'nicht_angemeldet', 'nicht_registriert'];

export class Live {
	connected = $state(false);
	/** Der Server verlangt die Windows-Anmeldung; der Kanal ist geschlossen. */
	ohneAnmeldung = $state(false);
	/** Angemeldet, aber mit einem fremden Windows-Konto; der Kanal ist geschlossen. */
	fremdesKonto = $state(false);
	pc = $state<PcTick | null>(null);
	lastVorgang = $state<VorgangEvent | null>(null);
	/** Zähler der Vorgangs-Ereignisse seit dem Start. */
	vorgaenge = $state(0);

	#source: EventSource | null = null;
	#listeners = new SvelteSet<ChangeListener>();
	#vorgangListeners = new SvelteSet<VorgangListener>();
	#nachholListeners = new SvelteSet<NachholListener>();
	#inhaltListeners = new SvelteSet<InhaltListener>();
	#kanalListeners = new SvelteSet<KanalListener>();
	#bereichListeners = new SvelteSet<BereichListener>();
	#bauListeners = new SvelteSet<BauListener>();
	#fortschrittListeners = new SvelteSet<BauFortschrittListener>();
	/** Nummer des zuletzt gesehenen Vorgangs-Ereignisses. */
	#letzteNr: number | null = null;
	/** War die Verbindung schon einmal weg? Dann beim nächsten `open` nachholen. */
	#getrennt = false;
	/** Wie viele eigene Versuche seit der letzten offenen Verbindung (für die Wartezeit). */
	#versuch = 0;
	/** Der geplante neue Versuch. */
	#timer: ReturnType<typeof setTimeout> | null = null;

	start(): void {
		if (this.#source !== null || typeof EventSource === 'undefined') return;
		this.#timerWeg();
		const stand = zugang.stand;
		if (stand === 'fehlt' || stand === 'fremd') {
			this.ohneAnmeldung = stand === 'fehlt';
			this.fremdesKonto = stand === 'fremd';
			this.#melden({ art: 'aufgegeben', stand });
			return;
		}
		const source = new EventSource(`${API}/events`);
		this.#source = source;
		this.ohneAnmeldung = false;
		this.fremdesKonto = false;
		source.onopen = () => {
			this.connected = true;
			this.#versuch = 0;
			zugang.setzen('da');
			// Offen heißt angemeldet: ein gemerkter Rückfall gilt nicht mehr.
			const warFrei = rueckfall.vergessen();
			if (this.#getrennt) {
				this.#getrennt = false;
				this.#nachholen('wiederverbunden');
			} else if (warFrei) {
				this.#nachholen('angemeldet');
			}
			this.#melden({ art: 'offen' });
		};
		source.onerror = () => {
			this.connected = false;
			this.#getrennt = true;
			// CONNECTING: Der Browser verbindet selbst neu; bis dahin „getrennt“.
			// CLOSED: Er hat aufgegeben (etwa 401); dann fragen wir nach.
			if (source.readyState === EventSource.CLOSED) void this.#nachAufgabe(source);
			this.#melden({ art: 'fehler' });
		};
		// Kopfleiste, Fassung 2: das Wort zum Anmeldestand, wie auf dem freien Kanal.
		source.addEventListener('anmeldung', (ev: MessageEvent<string>) => {
			const a = parse(ev.data) as AnmeldungEvent | null;
			const wort = a?.stand;
			if (typeof wort === 'string' && WOERTER.includes(wort)) {
				this.#melden({ art: 'anmeldung', wort: wort });
			}
		});
		source.addEventListener('fassung', (ev: MessageEvent<string>) => {
			const f = parse(ev.data) as FassungEvent | null;
			if (f !== null) this.#melden({ art: 'fassung', fassung: f });
		});
		source.addEventListener('pc', (ev: MessageEvent<string>) => {
			const tick = parse(ev.data) as PcTick | null;
			if (tick !== null) this.pc = tick;
		});
		source.addEventListener('change', (ev: MessageEvent<string>) => {
			const change = parse(ev.data) as ChangeEvent | null;
			if (change === null) return;
			for (const l of this.#listeners) l(change);
		});
		for (const name of ['projekt', 'daten', 'inhalt']) {
			source.addEventListener(name, (ev: MessageEvent<string>) => {
				const a = alsAenderung(name, parse(ev.data));
				if (a === null) return;
				for (const l of this.#inhaltListeners) l(a);
			});
		}
		// Live-Lücken: Datenspeicher, Benutzer-Daten, docs/.
		for (const name of BEREICH_EREIGNISSE) {
			source.addEventListener(name, (ev: MessageEvent<string>) => {
				const e = parse(ev.data) as BereichEvent | null;
				const b = e?.bereich;
				if (!istBereich(b)) return;
				for (const l of this.#bereichListeners) l(b);
			});
		}
		// Etappe 4c: Bau-Stand der Seiten live.
		source.addEventListener('bau', (ev: MessageEvent<string>) => {
			const b = parse(ev.data) as BauEvent | null;
			if (b === null || typeof b.slug !== 'string') return;
			for (const l of this.#bauListeners) l(b);
		});
		source.addEventListener('bau-fortschritt', (ev: MessageEvent<string>) => {
			const f = parse(ev.data) as BauFortschrittEvent | null;
			if (f === null || typeof f.fertig !== 'number' || typeof f.gesamt !== 'number') return;
			for (const l of this.#fortschrittListeners) l(f);
		});
		source.addEventListener('vorgang', (ev: MessageEvent<string>) => {
			const vorgang = parse(ev.data) as VorgangEvent | null;
			if (vorgang === null) return;
			const fehlt = luecke(this.#letzteNr, vorgang.nr);
			this.#letzteNr = vorgang.nr;
			this.lastVorgang = vorgang;
			this.vorgaenge += 1;
			for (const l of this.#vorgangListeners) l(vorgang);
			if (fehlt) this.#nachholen('luecke');
		});
		source.addEventListener('nachholen', (ev: MessageEvent<string>) => {
			const n = parse(ev.data) as NachholEvent | null;
			// Nach dem Neuladen ist die nächste Nummer keine Lücke mehr.
			this.#letzteNr = null;
			this.#nachholen(n?.grund ?? 'ueberlauf');
		});
	}

	stop(): void {
		this.#timerWeg();
		this.#source?.close();
		this.#source = null;
		this.connected = false;
		this.#getrennt = false;
		this.#letzteNr = null;
		this.#versuch = 0;
		// Was aus dem geschützten Kanal kam, gilt ohne ihn nicht mehr (Abnahme
		// Kopfleiste 2): sonst zeigte /pc nach Abmelden oder Widerruf für immer den
		// letzten geschützten PC-Wert, und der Vorgang bliebe stehen.
		this.pc = null;
		this.lastVorgang = null;
	}

	#timerWeg(): void {
		if (this.#timer !== null) clearTimeout(this.#timer);
		this.#timer = null;
	}

	/**
	 * Der Browser hat die Verbindung aufgegeben: fehlt die Anmeldung oder ist es
	 * ein fremdes Konto, bleibt der Kanal zu; sonst später neu versuchen.
	 */
	async #nachAufgabe(source: EventSource): Promise<void> {
		const stand = await anmeldungPruefen();
		// Inzwischen gestoppt oder neu gestartet: nichts mehr tun.
		if (this.#source !== source) return;
		source.close();
		this.#source = null;
		this.connected = false;
		if (stand === 'fehlt' || stand === 'fremd') {
			zugang.setzen(stand);
			this.#getrennt = false;
			this.ohneAnmeldung = stand === 'fehlt';
			this.fremdesKonto = stand === 'fremd';
			this.#melden({ art: 'aufgegeben', stand });
			return;
		}
		// `#getrennt` bleibt gesetzt: Klappt der neue Versuch, wird nachgeholt.
		const ms = wartezeit(this.#versuch);
		this.#versuch += 1;
		this.#timer = setTimeout(() => {
			this.#timer = null;
			this.start();
		}, ms);
	}

	#melden(m: KanalMeldung): void {
		for (const l of this.#kanalListeners) l(m);
	}

	/** Für den freien Kanal: offen, Fehler, aufgegeben, `anmeldung`, `fassung`. */
	onKanal(listener: KanalListener): () => void {
		this.#kanalListeners.add(listener);
		return () => this.#kanalListeners.delete(listener);
	}

	/** Ist gerade eine Verbindung dieses Kanals offen oder im Aufbau? */
	get laeuft(): boolean {
		return this.#source !== null || this.#timer !== null;
	}

	#nachholen(grund: NachholGrund): void {
		for (const l of this.#nachholListeners) l(grund);
	}

	/** Meldet sich bei jeder Änderung; gibt die Abmeldung zurück. */
	onChange(listener: ChangeListener): () => void {
		this.#listeners.add(listener);
		return () => this.#listeners.delete(listener);
	}

	/**
	 * Meldet sich bei jeder Änderung am Inhalt (`projekt`, `daten`, `inhalt`,
	 * Etappe 2d); gibt die Abmeldung zurück.
	 */
	onInhalt(listener: InhaltListener): () => void {
		this.#inhaltListeners.add(listener);
		return () => this.#inhaltListeners.delete(listener);
	}

	/**
	 * Meldet sich bei jedem Ereignis mit Bereich (`speicher`, `pc-benutzer`,
	 * `docs`; Live-Lücken); gibt die Abmeldung zurück.
	 */
	onBereich(listener: BereichListener): () => void {
		this.#bereichListeners.add(listener);
		return () => this.#bereichListeners.delete(listener);
	}

	/** Meldet sich bei jedem neuen Vorgangs-Ereignis; gibt die Abmeldung zurück. */
	onVorgang(listener: VorgangListener): () => void {
		this.#vorgangListeners.add(listener);
		return () => this.#vorgangListeners.delete(listener);
	}

	/** Etappe 4c: meldet sich bei jedem Ereignis `bau`; gibt die Abmeldung zurück. */
	onBau(listener: BauListener): () => void {
		this.#bauListeners.add(listener);
		return () => this.#bauListeners.delete(listener);
	}

	/** Zweig C: meldet sich bei jedem Ereignis `bau-fortschritt`; gibt die Abmeldung zurück. */
	onBauFortschritt(listener: BauFortschrittListener): () => void {
		this.#fortschrittListeners.add(listener);
		return () => this.#fortschrittListeners.delete(listener);
	}

	/** Meldet sich, wenn Ereignisse verpasst sein können; gibt die Abmeldung zurück. */
	onNachholen(listener: NachholListener): () => void {
		this.#nachholListeners.add(listener);
		return () => this.#nachholListeners.delete(listener);
	}
}

export const live = new Live();
