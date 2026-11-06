/**
 * Rückfall auf den freien Weg (Auflage aus 3a-4): Er gilt höchstens 5
 * Minuten und fällt bei einem Anmelde-Ereignis (Live-Kanal offen) sofort
 * weg. Getestet mit falschen Zeitgebern.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { FRIST_MS, Rueckfall } from './rueckfall.ts';

afterEach(() => {
	vi.useRealTimers();
});

describe('Rueckfall', () => {
	it('gilt nach merken, bis die Frist um ist; danach wieder /voll', () => {
		vi.useFakeTimers({ toFake: ['Date'] });
		vi.setSystemTime(new Date(2026, 9, 30, 12, 0));
		const r = new Rueckfall();
		expect(r.gemerkt('pc-verlauf')).toBe(false);
		r.merken('pc-verlauf');
		expect(r.gemerkt('pc-verlauf')).toBe(true);
		expect(r.gemerkt('projekt-stand')).toBe(false);
		vi.advanceTimersByTime(FRIST_MS - 1);
		expect(r.gemerkt('pc-verlauf')).toBe(true);
		vi.advanceTimersByTime(1);
		expect(r.gemerkt('pc-verlauf')).toBe(false);
		// Neues 401: wieder gemerkt, mit neuer Frist.
		r.merken('pc-verlauf');
		vi.advanceTimersByTime(FRIST_MS / 2);
		expect(r.gemerkt('pc-verlauf')).toBe(true);
	});

	it('vergessen (Anmelde-Ereignis): sofort weg; sagt, ob etwas galt', () => {
		vi.useFakeTimers({ toFake: ['Date'] });
		const r = new Rueckfall();
		expect(r.vergessen()).toBe(false);
		r.merken('pc-verlauf');
		r.merken('projekt-stand');
		expect(r.vergessen()).toBe(true);
		expect(r.gemerkt('pc-verlauf')).toBe(false);
		expect(r.gemerkt('projekt-stand')).toBe(false);
		// Ein abgelaufener Rückfall zählt nicht als „galt“.
		r.merken('pc-verlauf');
		vi.advanceTimersByTime(FRIST_MS);
		expect(r.vergessen()).toBe(false);
	});
});
