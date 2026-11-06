/**
 * Etappe 4c: der Bau-Stand im Browser (`BauLage`): Liste, Ereignis `bau`,
 * Sätze nur angemeldet. Seitdem
 * bestellt der Browser keinen Bau mehr (kein Kreispfeil, kein POST).
 */
import { describe, expect, it } from 'vitest';
import type { ProjectSummary } from '$vertrag';
import { BauLage } from './bau.svelte.ts';

interface Aufruf {
	url: string;
	methode: string;
}

function antwort(status: number, body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

function fakeFetch(aufrufe: Aufruf[], antworte: (url: string, methode: string) => Response) {
	return ((url: string | URL | Request, init?: RequestInit) => {
		const adresse = typeof url === 'string' ? url : url instanceof URL ? url.href : url.url;
		const methode = init?.method ?? 'GET';
		aufrufe.push({ url: adresse, methode });
		return Promise.resolve(antworte(adresse, methode));
	}) as typeof fetch;
}

function projekt(slug: string, bau?: ProjectSummary['bau']): ProjectSummary {
	return {
		slug,
		title: slug,
		status: 'aktiv',
		bedeutung: 'laeuft',
		last_change: '2026-10-31',
		summary: '',
		chat: null,
		backup: null,
		backup_bedeutung: null,
		backup_note: '',
		vorschau: null,
		order: 1,
		open: [],
		repos: [],
		file_modified: null,
		...(bau === undefined ? {} : { bau })
	};
}

const GUT = new Date(2026, 9, 31, 14, 5).toISOString();

describe('BauLage (Etappe 4c)', () => {
	it('die Liste ist der Stand des Servers; ein Ereignis gilt bis zur nächsten Liste', () => {
		const l = new BauLage(fakeFetch([], () => antwort(401, {})));
		l.liste([
			projekt('a', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT }),
			projekt('ohne')
		]);
		expect(l.anzeige('a')?.zustand).toBe('gruen');
		expect(l.anzeige('ohne')).toBeNull();
		l.ereignis({ slug: 'a', zustand: 'laeuft', meldung: 'x', gescheitert: false });
		expect(l.anzeige('a')?.zustand).toBe('laeuft');
		l.liste([projekt('a', { zustand: 'rot', letzte_gute: GUT, gebaut_um: GUT })]);
		expect(l.anzeige('a')?.zustand).toBe('rot');
		expect(l.anzeige('a')?.zeile).toMatch(/^Seite mit Fehler: zeigt die letzte gute Fassung von /);
	});

	it('ohne Anmeldung (401): keine Sätze', async () => {
		const aufrufe: Aufruf[] = [];
		const l = new BauLage(fakeFetch(aufrufe, () => antwort(401, { art: 'anmeldung' })));
		await l.laden();
		expect(l.saetze).toEqual({});
		expect(aufrufe).toEqual([{ url: '/api/bauen', methode: 'GET' }]);
	});

	it('angemeldet: der Satz bei rot kommt aus /api/bauen; bestellt wird nichts', async () => {
		const aufrufe: Aufruf[] = [];
		const l = new BauLage(
			fakeFetch(aufrufe, () =>
				antwort(200, {
					projekte: [
						{
							slug: 'a',
							zustand: 'rot',
							letzte_gute: GUT,
							satz: 'Die neue Fassung hat einen Fehler.'
						},
						{ slug: 'b', zustand: 'gruen', letzte_gute: GUT, satz: null }
					]
				})
			)
		);
		l.liste([projekt('a', { zustand: 'rot', letzte_gute: GUT, gebaut_um: GUT })]);
		await l.laden();
		expect(l.saetze).toEqual({ a: 'Die neue Fassung hat einen Fehler.' });
		expect(l.anzeige('a')?.satz).toBe('Die neue Fassung hat einen Fehler.');
		// Der Browser bestellt keinen Bau.
		expect(aufrufe.every((a) => a.methode === 'GET')).toBe(true);
		expect('bestellen' in l).toBe(false);
	});

	it('ein Netzfehler lässt die Sätze, wie sie sind', async () => {
		const l = new BauLage(() => Promise.reject(new TypeError('Failed to fetch')));
		l.saetze = { a: 'alt' };
		await l.laden();
		expect(l.saetze).toEqual({ a: 'alt' });
	});
});
