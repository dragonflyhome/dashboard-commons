import { flushSync } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Abgeleitet, DatenAntwort, Gantt, ProjektAntwort } from '$vertrag';
import type { InhaltAenderung } from '../live.svelte.ts';
import { Zugang } from '../zugang.ts';
import {
	GESCHUETZTE_DATEN,
	MIT_ANSTEHEND,
	SATZ_DATEN_OHNE_ANMELDUNG,
	beobachten,
	datenAdresse,
	kopfAdresse,
	ladeDaten,
	ladeKopf,
	type Lader,
	type Umgebung
} from './laden.svelte.ts';

const abgeleitet: Abgeleitet = { heute: '2026-10-27', listen: {} };
const gantt: Gantt = {
	start: null,
	balken: [
		{ name: 'Kompost ansetzen', von: '2031-03', bis: '2031-04', farbe: 'laeuft', gruppe: null }
	]
};

function antwort(status: number, body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

/** Live-Kanal zum Auslösen von Hand. */
function testUmgebung(fetchFn: typeof fetch): Umgebung & {
	aendern(slug: string): void;
	anstehend(slug: string): void;
	sammeln(): void;
	nachholen(): void;
	hoerer(): number;
} {
	const change = new Set<(a: InhaltAenderung) => void>();
	const nach = new Set<() => void>();
	const anst = new Set<{ slug: string; l: () => void }>();
	return {
		fetch: fetchFn,
		onAnstehend: (slug, l) => {
			const e = { slug, l };
			anst.add(e);
			return () => anst.delete(e);
		},
		anstehend(slug) {
			for (const e of anst) if (e.slug === slug) e.l();
		},
		onInhalt: (l) => {
			change.add(l);
			return () => change.delete(l);
		},
		onNachholen: (l) => {
			nach.add(l);
			return () => nach.delete(l);
		},
		aendern(slug) {
			for (const l of change) l({ projekte: [slug], daten: [], version: 2, sammel: false });
		},
		sammeln() {
			for (const l of change)
				l({ projekte: ['a', 'b', 'c', 'd'], daten: [], version: 3, sammel: true });
		},
		nachholen() {
			for (const l of nach) l();
		},
		hoerer: () => change.size + nach.size + anst.size
	};
}

/** Wartet, bis alle Promises der Ladevorgänge durch sind. */
async function fertig(): Promise<void> {
	for (let i = 0; i < 5; i++) await Promise.resolve();
	await new Promise((r) => setTimeout(r, 0));
	flushSync();
}

function starten<T>(
	slug: () => string,
	umgebung: Umgebung,
	name = 'gantt'
): { lader: Lader<T>; ende: () => void } {
	let lader: Lader<T> | undefined;
	const ende = $effect.root(() => {
		lader = beobachten<T>(
			slug,
			(s) => datenAdresse(s, name),
			(json) => {
				const a = json as DatenAntwort;
				return { inhalt: a.inhalt as T, abgeleitet: a.abgeleitet };
			},
			umgebung,
			MIT_ANSTEHEND.includes(name),
			GESCHUETZTE_DATEN.includes(name)
		);
	});
	flushSync();
	if (lader === undefined) throw new Error('nicht gestartet');
	return { lader, ende };
}

describe('$projekt: ladeDaten', () => {
	it('lädt /api/projekte/<slug>/daten/<name>: erst laedt, dann fertig mit Inhalt und Abgeleitetem', async () => {
		const fetchFn = vi.fn(() =>
			Promise.resolve(antwort(200, { name: 'gantt', inhalt: gantt, abgeleitet }))
		);
		const u = testUmgebung(fetchFn);
		const { lader, ende } = starten<Gantt>(() => 'muster', u);
		expect(lader.zustand).toBe('laedt');
		expect(lader.inhalt).toBeNull();
		await fertig();
		expect(fetchFn).toHaveBeenCalledWith('/api/projekte/muster/daten/gantt', expect.anything());
		expect(lader.zustand).toBe('fertig');
		expect(lader.inhalt).toEqual(gantt);
		expect(lader.abgeleitet).toEqual(abgeleitet);
		expect(lader.satz).toBeNull();
		ende();
	});

	it('422: zustand fehler mit dem ganzen Satz des Servers (Datei und Zeile)', async () => {
		const satz = 'projects/muster/daten/gantt.toml, Zeile 3: Bei `bis` steht kein Datum.';
		const u = testUmgebung(() =>
			Promise.resolve(
				antwort(422, { fehler: satz, art: 'vertrag', datei: 'x', zeile: 3, satz: 'kurz' })
			)
		);
		const { lader, ende } = starten<Gantt>(() => 'muster', u);
		await fertig();
		expect(lader.zustand).toBe('fehler');
		expect(lader.satz).toBe(satz);
		expect(lader.inhalt).toBeNull();
		ende();
	});

	it('Etappe B2: 401 (nur mit Anmeldung, etwa die Fragen) ist kein Seitenfehler, nur „Anmelden“', async () => {
		let status = 401;
		const u = testUmgebung(() =>
			Promise.resolve(
				status === 200
					? antwort(200, { name: 'fragen', inhalt: { frage: [] }, abgeleitet })
					: antwort(status, { fehler: 'Bitte anmelden.', art: 'anmeldung' })
			)
		);
		const a = starten<unknown>(() => 'muster', u, 'fragen');
		await fertig();
		expect([a.lader.zustand, a.lader.ohneAnmeldung, a.lader.satz]).toEqual([
			'fehler',
			true,
			SATZ_DATEN_OHNE_ANMELDUNG
		]);
		status = 200;
		await a.lader.neuLaden();
		expect([a.lader.zustand, a.lader.ohneAnmeldung]).toEqual(['fertig', false]);
		a.ende();
	});

	it('Etappe B2: ohne Anmeldung kein fetch auf daten/fragen; nach der Anmeldung genau eins', async () => {
		const fetchFn = vi.fn(() =>
			Promise.resolve(antwort(200, { name: 'fragen', inhalt: { frage: [] }, abgeleitet }))
		);
		const z = new Zugang();
		z.setzen('fehlt');
		const angemeldet = new Set<() => void>();
		const u = {
			...testUmgebung(fetchFn),
			zugang: z,
			onAngemeldet: (l: () => void) => {
				angemeldet.add(l);
				return () => angemeldet.delete(l);
			}
		};
		const fragen = starten<unknown>(() => 'muster', u, 'fragen');
		const plan = starten<unknown>(() => 'muster', u, 'gantt');
		await fertig();
		const aufrufe = (teil: string): number =>
			fetchFn.mock.calls.filter((c: unknown[]) => String(c[0]).endsWith(teil)).length;
		// Geschützt: keine Anfrage, sofort der Satz zum Anmelden; freie Daten laden wie immer.
		expect([aufrufe('/fragen'), aufrufe('/gantt')]).toEqual([0, 1]);
		expect([fragen.lader.zustand, fragen.lader.ohneAnmeldung, fragen.lader.satz]).toEqual([
			'fehler',
			true,
			SATZ_DATEN_OHNE_ANMELDUNG
		]);
		// Die Anmeldung kommt über den Live-Kanal: genau eine Anfrage.
		z.setzen('da');
		for (const l of angemeldet) l();
		await fertig();
		expect([aufrufe('/fragen'), aufrufe('/gantt')]).toEqual([1, 1]);
		expect([fragen.lader.zustand, fragen.lader.ohneAnmeldung]).toEqual(['fertig', false]);
		// Noch ein Anmelde-Ereignis: angemeldet ist schon geladen, keine zweite Anfrage.
		for (const l of angemeldet) l();
		await fertig();
		expect(aufrufe('/fragen')).toBe(1);
		fragen.ende();
		plan.ende();
		expect(angemeldet.size).toBe(0);
	});

	it('Etappe B2: fragen lädt auch bei Anstehend und Vorgängen des Projekts neu, gantt nicht', async () => {
		const fetchFn = vi.fn(() =>
			Promise.resolve(antwort(200, { name: 'x', inhalt: { frage: [] }, abgeleitet }))
		);
		const u = testUmgebung(fetchFn);
		const fragen = starten<unknown>(() => 'muster', u, 'fragen');
		const plan = starten<unknown>(() => 'muster', u, 'gantt');
		await fertig();
		const aufrufe = (teil: string): number =>
			fetchFn.mock.calls.filter((c: unknown[]) => String(c[0]).endsWith(teil)).length;
		expect([aufrufe('/fragen'), aufrufe('/gantt')]).toEqual([1, 1]);
		u.anstehend('muster');
		await fertig();
		expect([aufrufe('/fragen'), aufrufe('/gantt')]).toEqual([2, 1]);
		u.anstehend('anderes');
		await fertig();
		expect(aufrufe('/fragen')).toBe(2);
		fragen.ende();
		plan.ende();
		expect(u.hoerer()).toBe(0);
	});

	it('404: zustand fehler mit dem Satz aus der Antwort; ohne Satz ein eigener', async () => {
		let body: unknown = { fehler: 'Keine Datei daten/gantt.toml', art: 'nicht-gefunden' };
		const u = testUmgebung(() => Promise.resolve(antwort(404, body)));
		const a = starten<Gantt>(() => 'muster', u);
		await fertig();
		expect(a.lader.zustand).toBe('fehler');
		expect(a.lader.satz).toBe('Keine Datei daten/gantt.toml');
		body = 'kein JSON-Objekt';
		await a.lader.neuLaden();
		expect(a.lader.satz).toBe('Diese Daten gibt es nicht.');
		a.ende();
	});

	it('Netzfehler: zustand fehler mit einfachem Satz', async () => {
		const u = testUmgebung(() => Promise.reject(new TypeError('Failed to fetch')));
		const { lader, ende } = starten<Gantt>(() => 'muster', u);
		await fertig();
		expect(lader.zustand).toBe('fehler');
		expect(lader.satz).toMatch(/Server antwortet gerade nicht/);
		ende();
	});

	it('lädt bei projekt für das eigene Projekt neu, bei anderen nicht; der alte Stand bleibt sichtbar', async () => {
		let n = 0;
		const fetchFn = vi.fn(() => {
			n += 1;
			const inhalt: Gantt = { ...gantt, start: n === 1 ? null : '2026-11' };
			return Promise.resolve(antwort(200, { name: 'gantt', inhalt, abgeleitet }));
		});
		const u = testUmgebung(fetchFn);
		const { lader, ende } = starten<Gantt>(() => 'muster', u);
		await fertig();
		expect(fetchFn).toHaveBeenCalledTimes(1);

		u.aendern('anderes');
		u.aendern('muster-zwei');
		await fertig();
		expect(fetchFn).toHaveBeenCalledTimes(1);

		u.aendern('muster');
		// Während des Neuladens bleibt der alte Inhalt stehen.
		expect(lader.zustand).toBe('fertig');
		expect(lader.inhalt?.start).toBeNull();
		await fertig();
		expect(fetchFn).toHaveBeenCalledTimes(2);
		expect(lader.inhalt?.start).toBe('2026-11');
		// Ein Sammelereignis (viele Projekte auf einmal) lädt auch neu.
		u.sammeln();
		await fertig();
		expect(fetchFn).toHaveBeenCalledTimes(3);
		ende();
	});

	it('lädt beim Nachholen neu und meldet sich beim Abbau ab', async () => {
		const fetchFn = vi.fn(() =>
			Promise.resolve(antwort(200, { name: 'gantt', inhalt: gantt, abgeleitet }))
		);
		const u = testUmgebung(fetchFn);
		const { ende } = starten<Gantt>(() => 'muster', u);
		await fertig();
		expect(u.hoerer()).toBe(2);
		u.nachholen();
		await fertig();
		expect(fetchFn).toHaveBeenCalledTimes(2);
		ende();
		expect(u.hoerer()).toBe(0);
	});

	it('neues Projekt im Kontext: neue Adresse, zurück auf laedt; alte Antworten zählen nicht', async () => {
		const offen: ((r: Response) => void)[] = [];
		const fetchFn = vi.fn(
			() =>
				new Promise<Response>((r) => {
					offen.push(r);
				})
		);
		const u = testUmgebung(fetchFn);
		let slug = $state('alpha');
		const { lader, ende } = starten<Gantt>(() => slug, u);
		slug = 'beta';
		flushSync();
		expect(fetchFn).toHaveBeenLastCalledWith('/api/projekte/beta/daten/gantt', expect.anything());
		expect(lader.zustand).toBe('laedt');
		// Die späte Antwort für alpha darf beta nicht überschreiben.
		offen[0]?.(antwort(200, { name: 'gantt', inhalt: { ...gantt, start: '1999-01' }, abgeleitet }));
		await fertig();
		expect(lader.zustand).toBe('laedt');
		offen[1]?.(antwort(200, { name: 'gantt', inhalt: gantt, abgeleitet }));
		await fertig();
		expect(lader.zustand).toBe('fertig');
		expect(lader.inhalt).toEqual(gantt);
		ende();
	});

	it('Adressen kodieren Slug und Namen', () => {
		expect(datenAdresse('muster', 'eigen-notizen')).toBe(
			'/api/projekte/muster/daten/eigen-notizen'
		);
		expect(datenAdresse('a b', 'x/y')).toBe('/api/projekte/a%20b/daten/x%2Fy');
		expect(kopfAdresse('muster')).toBe('/api/projekte/muster');
	});

	it('außerhalb einer Projektseite: ein verständlicher Fehler', () => {
		expect(() => ladeDaten('gantt')).toThrow(/geht nur in einer Projektseite/);
		expect(() => ladeKopf()).toThrow(/ladeKopf geht nur/);
	});

	it('Kopf: ProjektAntwort ohne Abgeleitetes', async () => {
		const kopf = {
			slug: 'muster',
			vorschau: false,
			daten: ['gantt']
		} as unknown as ProjektAntwort;
		const u = testUmgebung(() => Promise.resolve(antwort(200, kopf)));
		let lader: Lader<ProjektAntwort> | undefined;
		const ende = $effect.root(() => {
			lader = beobachten<ProjektAntwort>(
				() => 'muster',
				kopfAdresse,
				(json) => ({ inhalt: json as ProjektAntwort, abgeleitet: null }),
				u
			);
		});
		flushSync();
		await fertig();
		expect(lader?.zustand).toBe('fertig');
		expect(lader?.inhalt?.daten).toEqual(['gantt']);
		expect(lader?.abgeleitet).toBeNull();
		ende();
	});
});
