import { describe, expect, it } from 'vitest';
import {
	clampView,
	dateToDay,
	dayToDate,
	fmtDay,
	fmtDuration,
	fraction,
	pan,
	padded,
	ticks,
	todayDays,
	zoom
} from './timescale';

describe('timescale', () => {
	it('rechnet wie chrono num_days_from_ce', () => {
		// 2026-11-01 ist laut Rust-API Tag 739921.
		expect(dateToDay(new Date(Date.UTC(2026, 10, 1)))).toBe(739_921);
		expect(fmtDay(739_921)).toBe('01.11.2026');
		expect(dayToDate(719_163).toISOString()).toBe('1970-01-01T00:00:00.000Z');
		const t = todayDays(new Date(2026, 9, 25, 23, 30));
		expect(fmtDay(t)).toBe('25.10.2026');
	});

	it('zoomt um einen Ankerpunkt und verschiebt', () => {
		const v = { min: 100, max: 200 };
		expect(zoom(v, 2, 0.5)).toEqual({ min: 125, max: 175 });
		expect(zoom(v, 2, 0)).toEqual({ min: 100, max: 150 });
		expect(pan(v, 0.1)).toEqual({ min: 110, max: 210 });
		expect(fraction(v, 150)).toBe(0.5);
	});

	it('hält das Fenster in den Grenzen', () => {
		const bounds = { min: 0, max: 100 };
		expect(clampView({ min: -20, max: 30 }, bounds, 5)).toEqual({ min: 0, max: 50 });
		expect(clampView({ min: 80, max: 130 }, bounds, 5)).toEqual({ min: 50, max: 100 });
		expect(clampView({ min: 40, max: 41 }, bounds, 10)).toEqual({ min: 40, max: 50 });
		expect(clampView({ min: -50, max: 300 }, bounds, 5)).toEqual({ min: 0, max: 100 });
		expect(padded(bounds, 0.1)).toEqual({ min: -10, max: 110 });
	});

	it('wählt Marken nach Breite', () => {
		const nov1 = 739_921;
		const days = ticks({ min: nov1, max: nov1 + 6 }, true);
		expect(days.map((t) => t.label)).toEqual([
			'01.11.',
			'02.11.',
			'03.11.',
			'04.11.',
			'05.11.',
			'06.11.',
			'07.11.'
		]);
		expect(days[0]?.major).toBe(true);
		const weeks = ticks({ min: nov1, max: nov1 + 60 }, true);
		expect(weeks.length).toBeGreaterThan(5);
		expect(weeks.every((t) => dayToDate(t.at).getUTCDay() === 1)).toBe(true);
		const months = ticks({ min: nov1, max: nov1 + 240 }, true);
		expect(months[0]?.label).toBe('Nov 2026');
		expect(months.some((t) => t.label === 'Jan 2027' && t.major)).toBe(true);
		const years = ticks({ min: nov1, max: nov1 + 365 * 6 }, true);
		expect(years.map((t) => t.label)).toEqual(['2027', '2028', '2029', '2030', '2031', '2032']);
		const plan = ticks({ min: 0, max: 14 }, false);
		expect(plan.map((t) => t.label)).toEqual(['M0', 'M2', 'M4', 'M6', 'M8', 'M10', 'M12', 'M14']);
	});

	it('clampView verwirft nicht endliche Fenster (NaN, ±Infinity)', () => {
		const bounds = { min: 0, max: 100 };
		const prev = { min: 20, max: 40 };
		const bad = [
			{ min: NaN, max: NaN },
			{ min: 10, max: NaN },
			{ min: Infinity, max: Infinity },
			{ min: -Infinity, max: 50 },
			{ min: 10, max: Infinity }
		];
		for (const v of bad) {
			expect(clampView(v, bounds, 5, prev)).toEqual(prev);
			expect(clampView(v, bounds, 5)).toEqual(bounds);
		}
		// Vorige Ansicht selbst kaputt: volle Ansicht.
		expect(clampView({ min: NaN, max: 1 }, bounds, 5, { min: NaN, max: NaN })).toEqual(bounds);
		// min == max: auf minSpan aufgeweitet, endlich.
		expect(clampView({ min: 30, max: 30 }, bounds, 5)).toEqual({ min: 30, max: 35 });
	});

	it('ticks endet bei NaN, ±Infinity und min == max mit leerer Liste', () => {
		const t0 = Date.now();
		const views = [
			{ min: NaN, max: NaN },
			{ min: 739_921, max: NaN },
			{ min: NaN, max: 739_999 },
			{ min: 739_921, max: Infinity },
			{ min: -Infinity, max: 739_921 },
			{ min: -Infinity, max: Infinity },
			{ min: 739_921, max: 739_921 },
			{ min: 739_950, max: 739_921 }
		];
		for (const v of views) {
			expect(ticks(v, true)).toEqual([]);
			expect(ticks(v, false)).toEqual([]);
		}
		// Riesige, aber endliche Fenster laufen in die Sicherung statt endlos.
		expect(ticks({ min: 0, max: 1e15 }, true).length).toBeLessThanOrEqual(10_000);
		expect(ticks({ min: 0, max: 1e15 }, false).length).toBeLessThanOrEqual(10_000);
		expect(ticks({ min: 739_921, max: 739_921 + 800 }, true, NaN).length).toBeGreaterThan(0);
		expect(Date.now() - t0).toBeLessThan(2000);
	});

	it('beschreibt Dauern', () => {
		expect(fmtDuration(1, true)).toBe('1 Tag');
		expect(fmtDuration(21, true)).toBe('3 Wochen');
		expect(fmtDuration(120, true)).toBe('4 Monate');
		expect(fmtDuration(800, true)).toBe('2,2 Jahre');
		expect(fmtDuration(2, false)).toBe('2 Monate');
	});
});
