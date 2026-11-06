/**
 * Etappe 3a-4: `frist_stufe` hängt vom Tag ab. Eine offene Seite lädt
 * „Anstehend“ um Mitternacht (lokal) neu und bekommt so die neuen Stufen;
 * vorher bleibt die alte Liste stehen. Getestet mit falschen Zeitgebern.
 */
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { AnstehendListe, FristStufe } from '$vertrag';
import { beispielListe, eintrag, json } from './anstehend.beispiel.ts';
import { Anstehend } from './anstehend.svelte.ts';
import { PUFFER_MS } from './tageswechsel.ts';

const vorherTz = process.env.TZ;
beforeAll(() => {
	process.env.TZ = 'Europe/Berlin';
});
afterAll(() => {
	if (vorherTz === undefined) delete process.env.TZ;
	else process.env.TZ = vorherTz;
});
afterEach(() => {
	vi.useRealTimers();
});

/** Was der Server an einem Tag liefert: eine Frist am 30.10. */
function liste(heute: string, stufe: FristStufe, tage: number): AnstehendListe {
	return {
		...beispielListe(),
		heute,
		eintraege: [eintrag({ id: 'a-frist', frist: '2026-10-30', tage_bis: tage, frist_stufe: stufe })]
	};
}

describe('Anstehend um Mitternacht', () => {
	it('lädt um 0:00 neu; die Frist für morgen ist danach heute', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		vi.setSystemTime(new Date(2026, 9, 29, 23, 59, 0));
		const holen = vi.fn(() => {
			const neu = new Date().getDate() === 30;
			return Promise.resolve(
				json(neu ? liste('2026-10-30', 'heute', 0) : liste('2026-10-29', 'bald', 1))
			);
		});
		const a = new Anstehend(holen);
		const ab = a.start();
		try {
			await vi.waitFor(() => {
				expect(a.liste?.eintraege[0]?.frist_stufe).toBe('bald');
			});
			expect(holen).toHaveBeenCalledTimes(1);
			// Eine Minute vor Mitternacht: noch nichts.
			await vi.advanceTimersByTimeAsync(59_000);
			expect(holen).toHaveBeenCalledTimes(1);
			expect(a.liste?.eintraege[0]?.frist_stufe).toBe('bald');
			// Mitternacht (plus Puffer, damit auch der Server schon den neuen Tag hat).
			await vi.advanceTimersByTimeAsync(1_000 + PUFFER_MS);
			expect(holen).toHaveBeenCalledTimes(2);
			await vi.waitFor(() => {
				expect(a.liste?.eintraege[0]?.frist_stufe).toBe('heute');
			});
			expect(a.liste?.heute).toBe('2026-10-30');
		} finally {
			ab();
		}
		// Nach der Abmeldung lädt nichts mehr.
		await vi.advanceTimersByTimeAsync(48 * 3_600_000);
		expect(holen).toHaveBeenCalledTimes(2);
	});
});
