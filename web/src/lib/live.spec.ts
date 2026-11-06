import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	alsAenderung,
	anmeldungPruefen,
	BEREICH_ABHAENGIGKEIT,
	bereichBetroffen,
	betroffen,
	betrifftProjekt,
	Live,
	luecke,
	NEU_VERSUCHEN_MS,
	vorgangBetroffen,
	wartezeit,
	type InhaltAenderung,
	type NachholGrund
} from './live.svelte.ts';
import { rueckfall } from './rueckfall.ts';
import { zugang } from './zugang.ts';
import type { LiveBereich } from '$vertrag';

describe('luecke', () => {
	it('erkennt fehlende Nummern', () => {
		expect(luecke(41, 42)).toBe(false);
		expect(luecke(41, 43)).toBe(true);
		expect(luecke(41, 100)).toBe(true);
	});
	it('ohne vorherige Nummer oder bei kleinerer Nummer keine Lücke', () => {
		expect(luecke(null, 1042)).toBe(false);
		expect(luecke(1042, 1042)).toBe(false);
		expect(luecke(1042, 3)).toBe(false);
	});
});

/** Kleiner Ersatz für EventSource: Ereignisse lassen sich von Hand auslösen. */
class FakeSource {
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSED = 2;
	static letzte: FakeSource | null = null;
	static anzahl = 0;
	readyState = FakeSource.CONNECTING;
	onopen: (() => void) | null = null;
	onerror: (() => void) | null = null;
	#handler = new Map<string, (ev: MessageEvent<string>) => void>();
	constructor() {
		FakeSource.letzte = this;
		FakeSource.anzahl += 1;
	}
	/** Fehler, nach dem der Browser selbst neu verbindet (Netz weg). */
	fehlerNeuVerbinden(): void {
		this.readyState = FakeSource.CONNECTING;
		this.onerror?.();
	}
	/** Fehler, nach dem der Browser aufgibt (etwa 401). */
	fehlerAufgegeben(): void {
		this.readyState = FakeSource.CLOSED;
		this.onerror?.();
	}
	addEventListener(name: string, fn: (ev: MessageEvent<string>) => void): void {
		this.#handler.set(name, fn);
	}
	geschlossen = false;
	close(): void {
		this.geschlossen = true;
	}
	senden(name: string, daten: unknown): void {
		this.#handler.get(name)?.({ data: JSON.stringify(daten) } as MessageEvent<string>);
	}
}

function vorgang(nr: number): Record<string, unknown> {
	return { id: `v-${nr}`, slug: 'dashboard', art: 'frage', zustand: 'offen', nr };
}

describe('Live: nachholen', () => {
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('meldet Lücken und das Wiederverbinden', () => {
		vi.stubGlobal('EventSource', FakeSource);
		const live = new Live();
		const gruende: NachholGrund[] = [];
		live.onNachholen((g) => gruende.push(g));
		live.start();
		const s = FakeSource.letzte;
		if (s === null) throw new Error('keine Quelle');

		s.onopen?.();
		s.senden('vorgang', vorgang(10));
		s.senden('vorgang', vorgang(11));
		expect(gruende).toEqual([]);
		s.senden('vorgang', vorgang(14));
		expect(gruende).toEqual(['luecke']);

		s.fehlerNeuVerbinden();
		expect(live.connected).toBe(false);
		s.onopen?.();
		expect(live.connected).toBe(true);
		expect(gruende).toEqual(['luecke', 'wiederverbunden']);
		s.onopen?.();
		expect(gruende).toHaveLength(2);
		live.stop();
	});

	it('meldet das Ereignis nachholen vom Server (Überlauf)', () => {
		vi.stubGlobal('EventSource', FakeSource);
		const live = new Live();
		const gruende: NachholGrund[] = [];
		live.onNachholen((g) => gruende.push(g));
		live.start();
		const s = FakeSource.letzte;
		if (s === null) throw new Error('keine Quelle');

		s.onopen?.();
		s.senden('vorgang', vorgang(10));
		s.senden('nachholen', { grund: 'ueberlauf' });
		expect(gruende).toEqual(['ueberlauf']);
		// Nach dem Überlauf zählt die nächste Nummer nicht noch einmal als Lücke.
		s.senden('vorgang', vorgang(40));
		expect(gruende).toEqual(['ueberlauf']);
		live.stop();
	});

	it('offen heißt angemeldet: ein gemerkter Rückfall fällt weg, die Seite lädt neu (3a+)', () => {
		vi.stubGlobal('EventSource', FakeSource);
		const live = new Live();
		const gruende: NachholGrund[] = [];
		live.onNachholen((g) => gruende.push(g));
		rueckfall.merken('pc-verlauf');
		live.start();
		const s = FakeSource.letzte;
		if (s === null) throw new Error('keine Quelle');
		s.onopen?.();
		expect(rueckfall.gemerkt('pc-verlauf')).toBe(false);
		expect(gruende).toEqual(['angemeldet']);
		// Ohne Rückfall öffnet der Kanal still.
		s.onopen?.();
		expect(gruende).toEqual(['angemeldet']);
		live.stop();
	});
});

/** fetch-Ersatz, der immer mit diesem Code (und optional JSON) antwortet, und zählt. */
function antwortMit(status: number, json?: unknown) {
	return vi.fn<(url: string) => Promise<Response>>(() =>
		Promise.resolve(
			new Response(json === undefined ? null : JSON.stringify(json), {
				status,
				headers: json === undefined ? {} : { 'content-type': 'application/json' }
			})
		)
	);
}

/** fetch-Ersatz, der der Reihe nach antwortet (eine Zahl: Code, `netz`: Netzfehler). */
function antwortenReihe(reihe: (number | 'netz')[]) {
	let i = 0;
	return vi.fn<(url: string) => Promise<Response>>(() => {
		const a = reihe[Math.min(i, reihe.length - 1)] ?? 'netz';
		i += 1;
		return a === 'netz'
			? Promise.reject(new TypeError('Failed to fetch'))
			: Promise.resolve(new Response(null, { status: a }));
	});
}

/** Lässt ausstehende Promises (fetch-Ersatz, Prüfung) durchlaufen. */
async function durchlaufen(): Promise<void> {
	for (let i = 0; i < 20; i++) await Promise.resolve();
}

function quelle(): FakeSource {
	const s = FakeSource.letzte;
	if (s === null) throw new Error('keine Quelle');
	return s;
}

describe('anmeldungPruefen', () => {
	it('401 fehlt, 403 fremdes-konto fremd, sonst (auch Netzfehler) sonst', async () => {
		const als = (f: unknown) => f as typeof fetch;
		expect(await anmeldungPruefen(als(antwortMit(401)))).toBe('fehlt');
		expect(
			await anmeldungPruefen(
				als(
					antwortMit(403, { fehler: 'Dieses Windows-Konto darf hier nicht.', art: 'fremdes-konto' })
				)
			)
		).toBe('fremd');
		expect(await anmeldungPruefen(als(antwortMit(403, { fehler: 'x', art: 'herkunft' })))).toBe(
			'sonst'
		);
		expect(await anmeldungPruefen(als(antwortMit(403)))).toBe('sonst');
		expect(await anmeldungPruefen(als(antwortMit(200)))).toBe('sonst');
		expect(await anmeldungPruefen(als(antwortMit(503)))).toBe('sonst');
		const kaputt = vi.fn(() => Promise.reject(new TypeError('Failed to fetch')));
		expect(await anmeldungPruefen(als(kaputt))).toBe('sonst');
	});
});

describe('wartezeit', () => {
	it('2 s, 5 s, 15 s, dann 30 s', () => {
		expect(NEU_VERSUCHEN_MS).toEqual([2000, 5000, 15000, 30000]);
		expect([0, 1, 2, 3, 4, 10].map(wartezeit)).toEqual([2000, 5000, 15000, 30000, 30000, 30000]);
	});
});

describe('Live: Anmeldung und neue Versuche', () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
		// Etappe 3b: der gemerkte Zugang gilt nur für einen Test.
		zugang.vergessen();
	});

	it('weiß die Seite schon, dass keine Anmeldung geht (3b), öffnet der Kanal nicht', () => {
		const quelle = vi.fn();
		vi.stubGlobal('EventSource', quelle);
		zugang.setzen('fehlt');
		const live = new Live();
		live.start();
		expect(quelle).not.toHaveBeenCalled();
		expect(live.ohneAnmeldung).toBe(true);
		zugang.setzen('fremd');
		const fremd = new Live();
		fremd.start();
		expect(fremd.fremdesKonto).toBe(true);
		expect(quelle).not.toHaveBeenCalled();
	});

	it('401 nach Aufgabe: Kanal zu, Zustand ohne Anmeldung', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		const f = antwortMit(401);
		vi.stubGlobal('fetch', f);
		const live = new Live();
		live.start();
		const s = quelle();

		s.fehlerAufgegeben();
		await vi.waitFor(() => {
			expect(live.ohneAnmeldung).toBe(true);
		});
		expect(live.fremdesKonto).toBe(false);
		expect(s.geschlossen).toBe(true);
		expect(live.connected).toBe(false);
		expect(f).toHaveBeenCalledTimes(1);
		expect(f.mock.calls[0]?.[0]).toBe('/api/vorgaenge/uebersicht');
		live.stop();
	});

	it('403 fremdes-konto nach Aufgabe: Kanal zu, Zustand fremdes Konto', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		vi.stubGlobal(
			'fetch',
			antwortMit(403, { fehler: 'Dieses Windows-Konto darf hier nicht.', art: 'fremdes-konto' })
		);
		const live = new Live();
		live.start();
		const s = quelle();

		s.fehlerAufgegeben();
		await vi.waitFor(() => {
			expect(live.fremdesKonto).toBe(true);
		});
		expect(live.ohneAnmeldung).toBe(false);
		expect(s.geschlossen).toBe(true);
		// Ein neuer Start (nach Ablauf des gemerkten Zugangs, 3b) setzt den Zustand zurück.
		zugang.vergessen();
		live.start();
		expect(live.fremdesKonto).toBe(false);
		live.stop();
	});

	it('CONNECTING: der Browser verbindet selbst, keine Nachfrage', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		const f = antwortMit(401);
		vi.stubGlobal('fetch', f);
		const live = new Live();
		live.start();
		const s = quelle();

		s.fehlerNeuVerbinden();
		s.fehlerNeuVerbinden();
		await durchlaufen();
		expect(f).not.toHaveBeenCalled();
		expect(live.connected).toBe(false);
		expect(s.geschlossen).toBe(false);
		s.onopen?.();
		expect(live.connected).toBe(true);
		live.stop();
	});

	it('Netzfehler, dann 401: erst neuer Versuch nach 2 s, dann ohne Anmeldung', async () => {
		vi.useFakeTimers();
		vi.stubGlobal('EventSource', FakeSource);
		const f = antwortenReihe(['netz', 401]);
		vi.stubGlobal('fetch', f);
		const live = new Live();
		const gruende: NachholGrund[] = [];
		live.onNachholen((g) => gruende.push(g));
		live.start();
		const erste = quelle();
		erste.onopen?.();

		erste.fehlerAufgegeben();
		await durchlaufen();
		expect(f).toHaveBeenCalledTimes(1);
		expect(erste.geschlossen).toBe(true);
		expect(live.ohneAnmeldung).toBe(false);
		await vi.advanceTimersByTimeAsync(1999);
		expect(quelle()).toBe(erste);
		await vi.advanceTimersByTimeAsync(1);
		const zweite = quelle();
		expect(zweite).not.toBe(erste);

		// Auch beim zweiten Fehler wird nachgefragt (nicht nur beim ersten je Start).
		zweite.fehlerAufgegeben();
		await durchlaufen();
		expect(f).toHaveBeenCalledTimes(2);
		expect(live.ohneAnmeldung).toBe(true);
		expect(zweite.geschlossen).toBe(true);
		// Danach kein weiterer Versuch.
		const vorher = FakeSource.anzahl;
		await vi.advanceTimersByTimeAsync(60_000);
		expect(FakeSource.anzahl).toBe(vorher);
		expect(gruende).toEqual([]);
		live.stop();
	});

	it('neue Versuche nach 2 s, 5 s, 15 s, dann alle 30 s; nach open wieder ab 2 s', async () => {
		vi.useFakeTimers();
		vi.stubGlobal('EventSource', FakeSource);
		vi.stubGlobal('fetch', antwortMit(503));
		const live = new Live();
		const gruende: NachholGrund[] = [];
		live.onNachholen((g) => gruende.push(g));
		live.start();

		/** Lässt die aktuelle Quelle aufgeben und misst, nach wie vielen ms die nächste kommt. */
		async function abstand(): Promise<number> {
			const alt = quelle();
			alt.fehlerAufgegeben();
			await durchlaufen();
			let ms = 0;
			while (quelle() === alt) {
				await vi.advanceTimersByTimeAsync(500);
				ms += 500;
				if (ms > 60_000) throw new Error('kein neuer Versuch');
			}
			return ms;
		}

		const reihe: number[] = [];
		for (let i = 0; i < 6; i++) reihe.push(await abstand());
		expect(reihe).toEqual([2000, 5000, 15000, 30000, 30000, 30000]);

		// Verbindung klappt: nachholen, und die Reihe beginnt von vorn.
		quelle().onopen?.();
		expect(live.connected).toBe(true);
		expect(gruende).toEqual(['wiederverbunden']);
		expect(await abstand()).toBe(2000);
		live.stop();
	});

	it('stop während des Wartens: kein neuer Versuch', async () => {
		vi.useFakeTimers();
		vi.stubGlobal('EventSource', FakeSource);
		vi.stubGlobal('fetch', antwortMit(503));
		const live = new Live();
		live.start();
		quelle().fehlerAufgegeben();
		await durchlaufen();
		const vorher = FakeSource.anzahl;
		live.stop();
		await vi.advanceTimersByTimeAsync(60_000);
		expect(FakeSource.anzahl).toBe(vorher);
	});

	it('gestoppt, bevor die Antwort kommt: nichts ändert sich', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		let antworten: (r: Response) => void = () => undefined;
		vi.stubGlobal(
			'fetch',
			vi.fn(
				() =>
					new Promise<Response>((r) => {
						antworten = r;
					})
			)
		);
		const live = new Live();
		live.start();
		quelle().fehlerAufgegeben();
		live.stop();
		antworten(new Response(null, { status: 401 }));
		await new Promise((r) => setTimeout(r, 0));
		expect(live.ohneAnmeldung).toBe(false);
	});
});

describe('Live: genaue Ereignisse (Etappe 2d)', () => {
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	const url = (h: string): URL => new URL(h);

	it('macht aus projekt, daten und inhalt eine Änderung', () => {
		expect(alsAenderung('projekt', { slug: 'alpha', version: 7 })).toEqual({
			projekte: ['alpha'],
			daten: [],
			version: 7,
			sammel: false
		});
		expect(alsAenderung('daten', { name: 'pc', version: 8 })).toEqual({
			projekte: [],
			daten: ['pc'],
			version: 8,
			sammel: false
		});
		expect(alsAenderung('inhalt', { version: 9, projekte: ['a', 'b'], daten: [] })?.sammel).toBe(
			true
		);
		expect(alsAenderung('projekt', null)).toBeNull();
		expect(alsAenderung('change', { slug: 'x' })).toBeNull();
	});

	it('lädt nur, was betroffen ist', () => {
		const alpha: InhaltAenderung = { projekte: ['alpha'], daten: [], version: 2, sammel: false };
		const f = betroffen(alpha);
		expect(f(url('api:projects'))).toBe(true);
		expect(f(url('api:project:alpha'))).toBe(true);
		expect(f(url('api:projekt:alpha'))).toBe(true);
		expect(f(url('api:project:beta'))).toBe(false);
		expect(f(url('api:pc'))).toBe(true);
		expect(f(url('api:vorgaenge'))).toBe(false);

		const pc = betroffen({
			projekte: [],
			daten: ['pc', 'schwerpunkte'],
			version: 3,
			sammel: false
		});
		expect(pc(url('api:pc'))).toBe(true);
		// Kopfleiste, Fassung 3: die Seitenleiste gruppiert nach data/schwerpunkte.toml.
		expect(pc(url('api:projects'))).toBe(true);
		expect(pc(url('api:schluessel'))).toBe(false);

		const alle = betroffen({ projekte: ['a', 'b', 'c', 'd'], daten: [], version: 4, sammel: true });
		expect(alle(url('api:project:zett'))).toBe(true);
		expect(alle(url('api:projekt:zett'))).toBe(true);
		expect(alle(url('api:schluessel'))).toBe(false);
		expect(betrifftProjekt(alpha, 'alpha')).toBe(true);
		expect(betrifftProjekt(alpha, 'beta')).toBe(false);
	});

	it('meldet die drei Ereignisse bei onInhalt, ein Sammelereignis einmal', () => {
		vi.stubGlobal('EventSource', FakeSource);
		const live = new Live();
		const gesehen: InhaltAenderung[] = [];
		const ab = live.onInhalt((a) => gesehen.push(a));
		live.start();
		const s = FakeSource.letzte;
		if (s === null) throw new Error('keine Quelle');
		s.senden('projekt', { slug: 'alpha', version: 5 });
		s.senden('daten', { name: 'pc', version: 6 });
		s.senden('inhalt', { version: 7, projekte: ['a', 'b', 'c', 'd', 'e'], daten: [] });
		expect(gesehen.map((a) => a.version)).toEqual([5, 6, 7]);
		expect(gesehen[2]?.projekte).toHaveLength(5);
		ab();
		s.senden('projekt', { slug: 'alpha', version: 8 });
		expect(gesehen).toHaveLength(3);
		live.stop();
	});
});

describe('Live: stop vergisst Geschütztes (Abnahme Kopfleiste 2)', () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		zugang.vergessen();
	});

	it('nach stop() sind PC-Werte und letzter Vorgang weg', () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('da');
		const live = new Live();
		live.start();
		const s = FakeSource.letzte;
		if (s === null) throw new Error('kein Kanal');
		s.senden('pc', { at: 'x', cpu_percent: 42 });
		s.senden('change', { area: 'projects', path: 'projects/a/projekt.toml', at: 'x' });
		s.senden('vorgang', vorgang(1));
		expect(live.pc?.cpu_percent).toBe(42);
		expect(live.lastVorgang).not.toBeNull();
		live.stop();
		expect(live.pc).toBeNull();
		expect(live.lastVorgang).toBeNull();
	});

	it('kennt kein lastChange mehr (Kleinigkeit aus der Abnahme live-luecken)', () => {
		const live = new Live();
		expect('lastChange' in live).toBe(false);
		expect('changes' in live).toBe(false);
	});
});

describe('Live-Lücken: Ereignisse mit Bereich und Assets', () => {
	afterEach(() => {
		vi.unstubAllGlobals();
	});
	const url = (h: string) => new URL(h);

	it('speicher, pc-benutzer und docs kommen bei onBereich an; Unbekanntes nicht', () => {
		vi.stubGlobal('EventSource', FakeSource);
		const live = new Live();
		const gesehen: LiveBereich[] = [];
		const ab = live.onBereich((b) => gesehen.push(b));
		live.start();
		const s = FakeSource.letzte;
		if (s === null) throw new Error('keine Quelle');
		s.senden('speicher', { bereich: 'aufgaben' });
		s.senden('pc-benutzer', { bereich: 'bote' });
		s.senden('docs', { bereich: 'entscheidungen' });
		s.senden('speicher', { bereich: 'gibt-es-nicht' });
		s.senden('speicher', null);
		expect(gesehen).toEqual(['aufgaben', 'bote', 'entscheidungen']);
		ab();
		s.senden('speicher', { bereich: 'meldungen' });
		expect(gesehen).toHaveLength(3);
		live.stop();
	});

	it('bereichBetroffen: je Bereich genau die passenden Schlüssel', () => {
		const daten = bereichBetroffen('aufgaben');
		expect(daten(url('api:daten'))).toBe(true);
		expect(daten(url('api:pc'))).toBe(false);
		expect(daten(url('api:vorgaenge'))).toBe(false);
		// Sicherung: speicher/vorgaenge lädt auch die Übersicht der Vorgänge.
		const vorgaenge = bereichBetroffen('vorgaenge');
		expect(vorgaenge(url('api:vorgaenge'))).toBe(true);
		expect(vorgaenge(url('api:daten'))).toBe(true);
		expect(vorgaenge(url('api:verlauf'))).toBe(true);
		// Tabellen der Projekte ohne Slug: nur /daten, keine Projektseite.
		const tabellen = bereichBetroffen('projekt-tabellen');
		expect(tabellen(url('api:daten'))).toBe(true);
		for (const k of ['api:projekt:architektur', 'api:project:architektur', 'api:projects']) {
			expect(tabellen(url(k)), k).toBe(false);
		}
		const win = bereichBetroffen('windows-ereignisse');
		expect(win(url('api:verlauf'))).toBe(true);
		expect(win(url('api:daten'))).toBe(true);
		expect(win(url('api:pc'))).toBe(false);
		expect(bereichBetroffen('inventar')(url('api:pc:benutzer'))).toBe(true);
		expect(bereichBetroffen('inventar')(url('api:pc'))).toBe(false);
		expect(bereichBetroffen('tresor')(url('api:schluessel'))).toBe(true);
		expect(bereichBetroffen('bote')(url('api:schluessel'))).toBe(true);
		expect(bereichBetroffen('entscheidungen')(url('api:entscheidungen'))).toBe(true);
		expect(bereichBetroffen('entscheidungen')(url('api:zielbild'))).toBe(false);
		expect(bereichBetroffen('zielbild')(url('api:zielbild'))).toBe(true);
		// Jeder Bereich aus dem Vertrag hat mindestens einen Schlüssel.
		for (const k of Object.values(BEREICH_ABHAENGIGKEIT)) expect(k.length).toBeGreaterThan(0);
	});

	it('vorgangBetroffen lädt nur die Übersicht der Vorgänge', () => {
		const v = vorgangBetroffen();
		expect(v(url('api:vorgaenge'))).toBe(true);
		for (const k of ['api:daten', 'api:pc', 'api:projects', 'api:entscheidungen']) {
			expect(v(url(k))).toBe(false);
		}
	});

	it('projekt mit assets: true nennt das Projekt in assets', () => {
		expect(alsAenderung('projekt', { slug: 'alpha', version: 9, assets: true })).toEqual({
			projekte: ['alpha'],
			daten: [],
			version: 9,
			sammel: false,
			assets: ['alpha']
		});
		expect(alsAenderung('projekt', { slug: 'alpha', version: 9 })?.assets).toBeUndefined();
	});

	it('projekt mit dateien nennt die geänderten Dateien (nur mit assets)', () => {
		expect(
			alsAenderung('projekt', { slug: 'alpha', version: 9, assets: true, dateien: ['bild.png'] })
		).toEqual({
			projekte: ['alpha'],
			daten: [],
			version: 9,
			sammel: false,
			assets: ['alpha'],
			assetDateien: ['bild.png']
		});
		expect(
			alsAenderung('projekt', { slug: 'alpha', version: 9, dateien: ['bild.png'] })?.assetDateien
		).toBeUndefined();
	});
});
