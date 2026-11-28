/**
 * Der gemeinsame Himmel-Laden (Kalender, Stufe 7b): Zählung der Nutzer, „häufig“
 * für /heute, Wahl des Blickorts mit #nr, Zeitgeber auf `naechster_tag`, neu laden
 * bei Bereich `blickort` und die Anzeigezone. Alle Werte erfunden.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { KalenderKopf, LiveBereich } from '$vertrag';
import { freierKanal } from './freikanal.svelte.ts';
import { HAEUFIG_MS, HimmelLaden, OHNE_ZONE_MS } from './himmel.svelte.ts';
import { live } from './live.svelte.ts';
import { anzeigeZone } from './zeit/zone.svelte.ts';

export const KOPF: KalenderKopf = {
	ort: 'Hütte am See',
	kennung: 'huette',
	folgt_heimat: true,
	datum: '2026-11-27',
	zone: 'Pacific/Auckland',
	aufgang: '06:01',
	untergang: '20:31',
	mittag: '13:16',
	aufgang_morgen: '06:00',
	hoehe: 41,
	hoehe_mittag: 66,
	naechster_tag: '2026-11-27T11:00:00Z',
	text: null,
	phase_grad: 120,
	phase: 'zunehmend',
	sued: true,
	naechster_vollmond: '2026-12-01'
};

function antwort(status: number, koerper: unknown): Response {
	return new Response(JSON.stringify(koerper), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

/** Ein offenes Versprechen, das der Test von außen erfüllt. */
function offen<T>(): { p: Promise<T>; erfuellen: (v: T) => void } {
	let erfuellen: (v: T) => void = () => undefined;
	const p = new Promise<T>((r) => (erfuellen = r));
	return { p, erfuellen };
}

afterEach(() => {
	vi.useRealTimers();
	vi.restoreAllMocks();
});

describe('Himmel-Laden: ein Laden für Kopfleiste und /heute', () => {
	it('zwei Nutzer: einer endet, der Kopf bleibt; der letzte endet, alles leer und Zone null', async () => {
		const f = vi.fn(() => Promise.resolve(antwort(200, KOPF)));
		const h = new HimmelLaden(f);
		const a = h.nutzen();
		const b = h.nutzen();
		await vi.waitFor(() => {
			expect(h.kopf?.kennung).toBe('huette');
		});
		// Der zweite Nutzer lädt nicht noch einmal.
		expect(f).toHaveBeenCalledTimes(1);
		expect(anzeigeZone()).toBe('Pacific/Auckland');
		a();
		a();
		expect(h.kopf?.kennung).toBe('huette');
		b();
		expect(h.kopf).toBeNull();
		expect(anzeigeZone()).toBeNull();
	});

	it('ein Fehler leert den Kopf, merkt den Fehler und setzt die Zone zurück', async () => {
		let status = 200;
		const f = vi.fn(() =>
			Promise.resolve(status === 200 ? antwort(200, KOPF) : antwort(404, { fehler: 'Kein Ort.' }))
		);
		const h = new HimmelLaden(f);
		const ab = h.nutzen();
		await vi.waitFor(() => {
			expect(anzeigeZone()).toBe('Pacific/Auckland');
		});
		status = 404;
		await h.laden();
		expect(h.kopf).toBeNull();
		expect(h.fehler).toMatchObject({ status: 404, message: 'Kein Ort.' });
		expect(anzeigeZone()).toBeNull();
		ab();
	});

	it('zwei Wahlen, die Antworten kommen umgekehrt: der Kopf zeigt die zweite; gesperrt, solange eine läuft', async () => {
		const erste = offen<Response>();
		const zweite = offen<Response>();
		const f = vi.fn((pfad: string, init?: RequestInit) => {
			if (pfad === '/api/kalender/kopf') return Promise.resolve(antwort(200, KOPF));
			const { ort } = JSON.parse(init?.body as string) as { ort: string | null };
			return ort === 'ort-1' ? erste.p : zweite.p;
		});
		const h = new HimmelLaden(f as typeof fetch);
		const ab = h.nutzen();
		await vi.waitFor(() => {
			expect(h.kopf).not.toBeNull();
		});
		const w1 = h.waehlen('ort-1');
		const w2 = h.waehlen(null);
		expect(h.waehlt).toBe(true);
		zweite.erfuellen(antwort(200, { ...KOPF, kennung: 'zuhause', ort: 'Zuhause', zone: null }));
		await w2;
		expect(h.waehlt).toBe(true);
		erste.erfuellen(antwort(200, { ...KOPF, kennung: 'ort-1', ort: 'Hafen' }));
		await w1;
		expect(h.waehlt).toBe(false);
		expect(h.kopf?.kennung).toBe('zuhause');
		expect(anzeigeZone()).toBeNull();
		const post = f.mock.calls.filter(([p]) => p === '/api/kalender/blickort');
		expect(post.map(([, i]) => [i?.method, i?.body])).toEqual([
			['POST', '{"ort":"ort-1"}'],
			['POST', '{"ort":null}']
		]);
		ab();
	});

	it('eine gescheiterte Wahl fliegt zum Aufrufer, der Kopf bleibt', async () => {
		const f = vi.fn((pfad: string) =>
			Promise.resolve(
				pfad === '/api/kalender/kopf'
					? antwort(200, KOPF)
					: antwort(422, { fehler: 'Diesen Ort gibt es nicht.' })
			)
		);
		const h = new HimmelLaden(f as typeof fetch);
		const ab = h.nutzen();
		await vi.waitFor(() => {
			expect(h.kopf).not.toBeNull();
		});
		await expect(h.waehlen('mars')).rejects.toThrow('Diesen Ort gibt es nicht.');
		expect(h.kopf?.kennung).toBe('huette');
		expect(h.waehlt).toBe(false);
		ab();
	});

	it('der Zeitgeber feuert zum gelieferten Beginn des nächsten Tags am Blickort', async () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-11-27T10:55:00Z'));
		const f = vi.fn(() => Promise.resolve(antwort(200, KOPF)));
		const h = new HimmelLaden(f);
		const ab = h.nutzen();
		await vi.advanceTimersByTimeAsync(0);
		expect(f).toHaveBeenCalledTimes(1);
		// Bis 11:00Z (Mitternacht in Auckland) nichts, danach genau ein Neuladen.
		await vi.advanceTimersByTimeAsync(5 * 60_000 - 1);
		expect(f).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(1000 + 1);
		expect(f).toHaveBeenCalledTimes(2);
		ab();
		expect(vi.getTimerCount()).toBe(0);
	});

	it('ohne IANA-Zone (naechster_tag leer) lädt er stündlich neu; der letzte Nutzer räumt den Zeitgeber ab', async () => {
		vi.useFakeTimers();
		// Lokal 10 Uhr: bis zur Mitternacht des Geräts sind es Stunden.
		vi.setSystemTime(new Date(2026, 10, 27, 10, 0));
		const f = vi.fn(() =>
			Promise.resolve(antwort(200, { ...KOPF, zone: null, naechster_tag: '' }))
		);
		const h = new HimmelLaden(f);
		const ab = h.nutzen();
		await vi.advanceTimersByTimeAsync(0);
		expect(f).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(OHNE_ZONE_MS - 1);
		expect(f).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(1);
		expect(f).toHaveBeenCalledTimes(2);
		await vi.advanceTimersByTimeAsync(OHNE_ZONE_MS);
		expect(f).toHaveBeenCalledTimes(3);
		ab();
		expect(vi.getTimerCount()).toBe(0);
	});

	it('mit IANA-Zone kein stündliches Neuladen', async () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-11-27T01:00:00Z'));
		const f = vi.fn(() => Promise.resolve(antwort(200, KOPF)));
		const h = new HimmelLaden(f);
		const ab = h.nutzen();
		await vi.advanceTimersByTimeAsync(2 * OHNE_ZONE_MS);
		expect(f).toHaveBeenCalledTimes(1);
		ab();
	});

	it('Abmelden leert den Kopf und setzt den Fehler „anmelden“ (401), ohne zu laden', async () => {
		const hoerer: (() => void)[] = [];
		vi.spyOn(freierKanal, 'onAbgemeldet').mockImplementation(
			(l) => (hoerer.push(l), () => undefined)
		);
		const f = vi.fn(() => Promise.resolve(antwort(200, KOPF)));
		const h = new HimmelLaden(f);
		const ab = h.nutzen();
		await vi.waitFor(() => {
			expect(h.kopf).not.toBeNull();
		});
		for (const l of hoerer) l();
		expect(h.kopf).toBeNull();
		expect(h.fehler).toMatchObject({ status: 401, art: 'anmeldung' });
		expect(anzeigeZone()).toBeNull();
		expect(f).toHaveBeenCalledTimes(1);
		ab();
	});

	it('Bereich „blickort“ lädt neu, andere Bereiche nicht', async () => {
		const hoerer: ((b: LiveBereich) => void)[] = [];
		vi.spyOn(live, 'onBereich').mockImplementation((l) => (hoerer.push(l), () => undefined));
		const f = vi.fn(() => Promise.resolve(antwort(200, KOPF)));
		const h = new HimmelLaden(f);
		const ab = h.nutzen();
		await vi.waitFor(() => {
			expect(f).toHaveBeenCalledTimes(1);
		});
		for (const l of hoerer) l('sicherung');
		expect(f).toHaveBeenCalledTimes(1);
		for (const l of hoerer) l('blickort');
		expect(f).toHaveBeenCalledTimes(2);
		ab();
	});

	it('„häufig“ lädt beim Kommen sofort und dann alle 10 Minuten; danach nur noch die Kopfleiste', async () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-11-27T01:00:00Z'));
		const f = vi.fn(() => Promise.resolve(antwort(200, KOPF)));
		const h = new HimmelLaden(f);
		const leiste = h.nutzen();
		await vi.advanceTimersByTimeAsync(HAEUFIG_MS);
		expect(f).toHaveBeenCalledTimes(1);
		const heute = h.nutzen(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(f).toHaveBeenCalledTimes(2);
		await vi.advanceTimersByTimeAsync(2 * HAEUFIG_MS);
		expect(f).toHaveBeenCalledTimes(4);
		heute();
		await vi.advanceTimersByTimeAsync(3 * HAEUFIG_MS);
		expect(f).toHaveBeenCalledTimes(4);
		expect(h.kopf).not.toBeNull();
		leiste();
	});
});
