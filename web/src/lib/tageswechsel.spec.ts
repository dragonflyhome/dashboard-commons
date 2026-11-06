import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { PUFFER_MS, beiTageswechsel, lokalerTag, msBisMitternacht } from './tageswechsel.ts';

const STUNDE = 3_600_000;

// Feste Zeitzone mit Sommerzeit, damit die Umstellungstage stimmen.
const vorherTz = process.env.TZ;
beforeAll(() => {
	process.env.TZ = 'Europe/Berlin';
});
afterAll(() => {
	if (vorherTz === undefined) delete process.env.TZ;
	else process.env.TZ = vorherTz;
});

describe('msBisMitternacht', () => {
	it('kurz vor Mitternacht: die Sekunden bis 0:00', () => {
		expect(msBisMitternacht(new Date(2026, 9, 29, 23, 59, 30))).toBe(30_000);
	});

	it('genau um Mitternacht: ein ganzer Tag, nie 0', () => {
		expect(msBisMitternacht(new Date(2026, 9, 29, 0, 0, 0))).toBe(24 * STUNDE);
	});

	it('landet immer auf der lokalen Mitternacht des nächsten Tages', () => {
		for (const d of [
			new Date(2026, 0, 31, 12, 0),
			new Date(2026, 11, 31, 23, 0),
			new Date(2028, 1, 28, 6)
		]) {
			const ziel = new Date(d.getTime() + msBisMitternacht(d));
			expect([ziel.getHours(), ziel.getMinutes(), ziel.getSeconds()]).toEqual([0, 0, 0]);
			expect(ziel.getDate()).not.toBe(d.getDate());
		}
	});

	it('Sommerzeit: der Tag der Umstellung hat 25 bzw. 23 Stunden', () => {
		// 25.10.2026: Ende der Sommerzeit (3:00 wird 2:00).
		expect(msBisMitternacht(new Date(2026, 9, 25, 0, 0))).toBe(25 * STUNDE);
		// 29.03.2026: Beginn der Sommerzeit (2:00 wird 3:00).
		expect(msBisMitternacht(new Date(2026, 2, 29, 0, 0))).toBe(23 * STUNDE);
		// Nach der Umstellung wie sonst.
		expect(msBisMitternacht(new Date(2026, 9, 25, 12, 0))).toBe(12 * STUNDE);
	});

	it('lokalerTag: JJJJ-MM-TT nach Ortszeit', () => {
		expect(lokalerTag(new Date(2026, 9, 30, 0, 0, 1))).toBe('2026-10-30');
		expect(lokalerTag(new Date(2026, 0, 5))).toBe('2026-01-05');
	});
});

/** Ein Dokument mit `visibilitychange` für den Test. */
function dokument(): EventTarget & { visibilityState: DocumentVisibilityState } {
	const d = new EventTarget() as EventTarget & { visibilityState: DocumentVisibilityState };
	d.visibilityState = 'visible';
	return d;
}

describe('beiTageswechsel', () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	it('ruft um Mitternacht, jeden Tag wieder; nach der Abmeldung nicht mehr', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date(2026, 9, 29, 23, 59, 0));
		const rufen = vi.fn();
		const doc = dokument();
		const ab = beiTageswechsel(rufen, undefined, doc);
		vi.advanceTimersByTime(60_000);
		expect(rufen).not.toHaveBeenCalled();
		vi.advanceTimersByTime(PUFFER_MS);
		expect(rufen).toHaveBeenCalledTimes(1);
		vi.advanceTimersByTime(24 * STUNDE);
		expect(rufen).toHaveBeenCalledTimes(2);
		ab();
		vi.advanceTimersByTime(48 * STUNDE);
		doc.dispatchEvent(new Event('visibilitychange'));
		expect(rufen).toHaveBeenCalledTimes(2);
		expect(vi.getTimerCount()).toBe(0);
	});

	it('nach dem Schlafen: wieder sichtbar an einem neuen Tag lädt neu, am selben Tag nicht', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date(2026, 9, 29, 22, 0, 0));
		const rufen = vi.fn();
		const doc = dokument();
		const ab = beiTageswechsel(rufen, undefined, doc);
		// Am selben Tag: nichts.
		vi.setSystemTime(new Date(2026, 9, 29, 23, 0, 0));
		doc.dispatchEvent(new Event('visibilitychange'));
		expect(rufen).not.toHaveBeenCalled();
		// Der PC schläft über Mitternacht, die Zeitgeber laufen nicht.
		vi.setSystemTime(new Date(2026, 9, 30, 7, 0, 0));
		doc.visibilityState = 'hidden';
		doc.dispatchEvent(new Event('visibilitychange'));
		expect(rufen).not.toHaveBeenCalled();
		doc.visibilityState = 'visible';
		doc.dispatchEvent(new Event('visibilitychange'));
		expect(rufen).toHaveBeenCalledTimes(1);
		// Der Zeitgeber steht neu auf der nächsten Mitternacht, nur einer.
		expect(vi.getTimerCount()).toBe(1);
		vi.advanceTimersByTime(17 * STUNDE + PUFFER_MS);
		expect(rufen).toHaveBeenCalledTimes(2);
		ab();
	});
});
