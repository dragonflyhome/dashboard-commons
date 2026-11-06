import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Verlauf, VerlaufHinweis, VerlaufHinweisArt } from '$vertrag';
import { ApiError } from '../api/client.ts';
import type * as Client from '../api/client.ts';
import { rueckfall } from '../rueckfall.ts';

const verlaufVoll = vi.fn<(seit?: string) => Promise<Verlauf>>();
const verlaufFrei = vi.fn<(seit?: string) => Promise<Verlauf>>();
vi.mock('../api/client.ts', async (original) => {
	const echt = await original<typeof Client>();
	return {
		...echt,
		api: {
			...echt.api,
			verlaufVoll: (seit?: string) => verlaufVoll(seit),
			verlauf: (seit?: string) => verlaufFrei(seit)
		}
	};
});

const { hinweiseFuerKasten, ladeVerlauf } = await import('./laden.ts');

describe('Hinweise im Kasten', () => {
	const hinweise: VerlaufHinweis[] = [
		{ art: 'beginnt-ab', text: 'Starts und Update-Ereignisse gibt es erst ab 2026-08-01.' },
		{ art: 'ohne-anmeldung', text: 'Ohne Anmeldung fehlen die Starts, Meldungen und Vorgänge.' }
	];

	it('ohne Anmeldung: der Hinweis der Art ohne-anmeldung fehlt, weil er schon sichtbar steht', () => {
		expect(hinweiseFuerKasten(hinweise, true)).toEqual([
			'Starts und Update-Ereignisse gibt es erst ab 2026-08-01.'
		]);
	});

	it('mit Anmeldung: alles bleibt', () => {
		expect(hinweiseFuerKasten(hinweise, false)).toEqual(hinweise.map((h) => h.text));
	});

	it('gefiltert wird nach der Art, nicht nach dem Satz', () => {
		const anders: VerlaufHinweis[] = [
			{ art: 'ohne-anmeldung', text: 'Ein ganz anderer Satz.' },
			{ art: 'ohne-speicher', text: 'Ohne Anmeldung beginnt dieser Satz, ist aber etwas anderes.' }
		];
		expect(hinweiseFuerKasten(anders, true)).toEqual([
			'Ohne Anmeldung beginnt dieser Satz, ist aber etwas anderes.'
		]);
	});

	it('eine unbekannte Art: der Satz steht unverändert da, nie verschluckt', () => {
		const neu = {
			art: 'neu-im-server' as VerlaufHinweisArt,
			text: 'Ein Hinweis, den die Oberfläche noch nicht kennt.'
		};
		for (const ohne of [true, false]) {
			expect(hinweiseFuerKasten([neu], ohne)).toEqual([
				'Ein Hinweis, den die Oberfläche noch nicht kennt.'
			]);
		}
	});
});

describe('Rückfall nach 401 oder 403 wird gemerkt (Auflage aus Hotfix 3a-1)', () => {
	const leer: Verlauf = { ereignisse: [], seit: '', bis: '', hinweise: [] };
	afterEach(() => {
		rueckfall.vergessen();
		verlaufVoll.mockReset();
		verlaufFrei.mockReset();
	});

	it.each([401, 403])('nach %i geht jede weitere Ladung gleich den freien Weg', async (status) => {
		verlaufVoll.mockRejectedValue(new ApiError(status, 'Nein.'));
		verlaufFrei.mockResolvedValue(leer);
		const f = (() => Promise.reject(new Error('nie'))) as typeof fetch;
		const erste = await ladeVerlauf(f, 7);
		expect(erste.ohneAnmeldung).toBe(true);
		expect(verlaufVoll).toHaveBeenCalledTimes(1);
		const zweite = await ladeVerlauf(f, 7);
		const dritte = await ladeVerlauf(f);
		expect([zweite.ohneAnmeldung, dritte.ohneAnmeldung]).toEqual([true, true]);
		expect(verlaufVoll).toHaveBeenCalledTimes(1);
		expect(verlaufFrei).toHaveBeenCalledTimes(3);
	});

	it('nach 5 Minuten versucht die nächste Ladung wieder /voll (Auflage aus 3a-4)', async () => {
		vi.useFakeTimers({ toFake: ['Date'] });
		try {
			verlaufVoll.mockRejectedValueOnce(new ApiError(401, 'Nein.')).mockResolvedValue(leer);
			verlaufFrei.mockResolvedValue(leer);
			const f = (() => Promise.reject(new Error('nie'))) as typeof fetch;
			expect((await ladeVerlauf(f)).ohneAnmeldung).toBe(true);
			vi.advanceTimersByTime(4 * 60_000);
			expect((await ladeVerlauf(f)).ohneAnmeldung).toBe(true);
			expect(verlaufVoll).toHaveBeenCalledTimes(1);
			vi.advanceTimersByTime(60_000);
			expect((await ladeVerlauf(f)).ohneAnmeldung).toBe(false);
			expect(verlaufVoll).toHaveBeenCalledTimes(2);
		} finally {
			vi.useRealTimers();
		}
	});

	it('ein Anmelde-Ereignis (vergessen) gilt sofort: wieder /voll', async () => {
		verlaufVoll.mockRejectedValueOnce(new ApiError(401, 'Nein.')).mockResolvedValue(leer);
		verlaufFrei.mockResolvedValue(leer);
		const f = (() => Promise.reject(new Error('nie'))) as typeof fetch;
		expect((await ladeVerlauf(f)).ohneAnmeldung).toBe(true);
		expect(rueckfall.vergessen()).toBe(true);
		expect((await ladeVerlauf(f)).ohneAnmeldung).toBe(false);
	});

	it('ein anderer Fehler (500) merkt nichts: die nächste Ladung versucht /voll wieder', async () => {
		verlaufVoll.mockRejectedValueOnce(new ApiError(500, 'Speicher zu.')).mockResolvedValue(leer);
		const f = (() => Promise.reject(new Error('nie'))) as typeof fetch;
		expect((await ladeVerlauf(f)).fehler).toBe('Speicher zu.');
		expect((await ladeVerlauf(f)).ohneAnmeldung).toBe(false);
		expect(verlaufVoll).toHaveBeenCalledTimes(2);
		expect(verlaufFrei).not.toHaveBeenCalled();
	});
});
