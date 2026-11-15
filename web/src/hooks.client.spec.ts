import { describe, expect, it, vi } from 'vitest';
import { handleError, init } from './hooks.client.ts';
import { istNetzfehler, SATZ_KEINE_VERBINDUNG } from '#lib/api/client.ts';
import { beiRueckkehrAuffrischen } from '#lib/zugang.ts';

// Schritt E2b: init meldet die Rückkehr in den Tab an und fragt einmal.
const { pruefen } = vi.hoisted(() => ({ pruefen: vi.fn(() => Promise.resolve('da' as const)) }));
vi.mock(import('#lib/zugang.ts'), async (echt) => {
	const m = await echt();
	m.zugang.pruefen = pruefen;
	return { ...m, beiRueckkehrAuffrischen: vi.fn(() => () => undefined) };
});

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

	it('Schritt E2b: init fragt einmal und frischt bei der Rückkehr in den Tab auf', async () => {
		await init();
		expect(beiRueckkehrAuffrischen).toHaveBeenCalledTimes(1);
		expect(pruefen).toHaveBeenCalledTimes(1);
	});
});
