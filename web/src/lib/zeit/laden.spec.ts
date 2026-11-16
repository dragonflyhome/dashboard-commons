import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Verlauf } from '$vertrag';
import { ApiError } from '../api/client.ts';
import type * as Client from '../api/client.ts';

const verlaufVoll = vi.fn<(seit?: string) => Promise<Verlauf>>();
vi.mock('../api/client.ts', async (original) => {
	const echt = await original<typeof Client>();
	return { ...echt, api: { ...echt.api, verlaufVoll: (seit?: string) => verlaufVoll(seit) } };
});

const { ladeVerlauf } = await import('./laden.ts');

describe('Verlauf nur über /voll (Schritt E3: kein freier Rückfall mehr)', () => {
	const leer: Verlauf = { ereignisse: [], seit: '', bis: '', hinweise: [] };
	const f = (() => Promise.reject(new Error('nie'))) as typeof fetch;
	afterEach(() => {
		verlaufVoll.mockReset();
	});

	it('geladen: der Verlauf, kein Fehler', async () => {
		verlaufVoll.mockResolvedValue(leer);
		expect(await ladeVerlauf(f, 7, new Date(2026, 10, 14))).toEqual({
			verlauf: leer,
			fehler: null
		});
		expect(verlaufVoll).toHaveBeenCalledWith('2026-11-08');
	});

	it.each([401, 403, 500])(
		'nach %i: fehler mit dem Satz und kein zweiter Abruf',
		async (status) => {
			verlaufVoll.mockRejectedValue(new ApiError(status, 'Nein.'));
			expect(await ladeVerlauf(f)).toEqual({ verlauf: null, fehler: 'Nein.' });
			expect(verlaufVoll).toHaveBeenCalledTimes(1);
		}
	);
});
