import { describe, expect, it, vi } from 'vitest';
import { handleError } from './hooks.client.ts';
import { istNetzfehler, SATZ_KEINE_VERBINDUNG } from '#lib/api/client.ts';

describe('Fehler im Browser (Auflage aus A2)', () => {
	it('Netzfehler aus Ladefunktionen stehen auf Deutsch da, nicht als „Failed to fetch“', () => {
		for (const satz of [
			'Failed to fetch',
			'NetworkError when attempting to fetch resource.',
			'Load failed'
		]) {
			expect(istNetzfehler(new TypeError(satz))).toBe(true);
		}
		expect(istNetzfehler(new Error('Failed to fetch'))).toBe(false);
		expect(istNetzfehler(new TypeError('x is not a function'))).toBe(false);
		const fehler = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		const ergebnis = handleError({
			kind: 'unknown',
			error: new TypeError('Failed to fetch')
		} as Parameters<typeof handleError>[0]);
		expect(ergebnis).toEqual({ status: 503, message: SATZ_KEINE_VERBINDUNG });
		expect(fehler).not.toHaveBeenCalled();
		fehler.mockRestore();
	});
});
