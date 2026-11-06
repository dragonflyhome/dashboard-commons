/**
 * Rechnen auf Zeitachsen für Gantt und Zeitstrahl.
 *
 * Einheiten wie im Rust-Backend: bei `dates` sind Werte Tage seit dem
 * 1.1. des Jahres 1 (chrono `num_days_from_ce`, 1 = 0001-01-01), sonst
 * Plan-Monate (0, 1, 2 …).
 */
import { istZahl } from './format.ts';

/** Tage seit 0001-01-01 für den 1970-01-01 (chrono-Zählung). */
export const EPOCH_DAYS = 719_163;
const MS_PER_DAY = 86_400_000;

export interface View {
	min: number;
	max: number;
}

/** Was TimeAxis seinem Inhalt reicht. */
export interface AxisArgs {
	x: (at: number) => number;
	view: View;
	width: number;
}

export interface Tick {
	at: number;
	label: string;
	/** Hauptmarke (z. B. Jahreswechsel), etwas kräftiger. */
	major: boolean;
}

export function dayToDate(days: number): Date {
	return new Date(Math.round((days - EPOCH_DAYS) * MS_PER_DAY));
}

export function dateToDay(d: Date): number {
	return Math.floor(d.getTime() / MS_PER_DAY) + EPOCH_DAYS;
}

/** Heute als Tageszahl (lokale Zeit, Mitternacht). */
export function todayDays(now: Date = new Date()): number {
	const local = new Date(now.getFullYear(), now.getMonth(), now.getDate());
	return dateToDay(new Date(Date.UTC(local.getFullYear(), local.getMonth(), local.getDate())));
}

export function span(v: View): number {
	return Math.max(v.max - v.min, 1e-9);
}

/** Anteil (0..1) eines Werts im Fenster. */
export function fraction(v: View, at: number): number {
	return (at - v.min) / span(v);
}

function finiteView(v: View | undefined): v is View {
	return v !== undefined && Number.isFinite(v.min) && Number.isFinite(v.max);
}

/**
 * Fenster auf die Grenzen beschneiden; nie kleiner als `minSpan`.
 * Nicht endliche Werte (NaN, ±Infinity, etwa aus 0/0 beim Ziehen ohne
 * Layout) werden verworfen: dann gilt das vorige Fenster `prev`, sonst die
 * vollen Grenzen. Sonst liefe ticks() endlos.
 */
export function clampView(v: View, bounds: View, minSpan: number, prev?: View): View {
	if (!finiteView(v) || !Number.isFinite(minSpan)) {
		return finiteView(prev)
			? { min: prev.min, max: prev.max }
			: { min: bounds.min, max: bounds.max };
	}
	if (!finiteView(bounds)) return { min: v.min, max: v.max };
	let s = Math.min(Math.max(span(v), minSpan), span(bounds));
	let min = v.min;
	if (min < bounds.min) min = bounds.min;
	if (min + s > bounds.max) min = bounds.max - s;
	if (s > span(bounds)) s = span(bounds);
	return { min, max: min + s };
}

/** Zoom um `factor` (>1 vergrößert) um den Punkt bei `anchor` (0..1). */
export function zoom(v: View, factor: number, anchor = 0.5): View {
	const s = span(v) / factor;
	const at = v.min + span(v) * anchor;
	const min = at - s * anchor;
	return { min, max: min + s };
}

/** Verschieben um einen Anteil des Fensters (0.1 = 10 % nach rechts). */
export function pan(v: View, deltaFraction: number): View {
	const d = span(v) * deltaFraction;
	return { min: v.min + d, max: v.max + d };
}

/** Grenzen mit etwas Luft links und rechts. */
export function padded(bounds: View, fractionEachSide = 0.04): View {
	const p = span(bounds) * fractionEachSide;
	return { min: bounds.min - p, max: bounds.max + p };
}

const MONTHS = ['Jan', 'Feb', 'Mär', 'Apr', 'Mai', 'Jun', 'Jul', 'Aug', 'Sep', 'Okt', 'Nov', 'Dez'];

/** Sicherung: mehr Durchläufe braucht keine Markenschleife. */
const MAX_TICK_LOOPS = 10_000;

function pad2(n: number): string {
	return n < 10 ? `0${n}` : `${n}`;
}

/** Marken für ein Fenster: Tage, Wochen, Monate, Quartale oder Jahre, je nach Breite. */
export function ticks(v: View, dates: boolean, maxTicks = 12): Tick[] {
	const out: Tick[] = [];
	if (!Number.isFinite(v.min) || !Number.isFinite(v.max) || v.max <= v.min) return out;
	if (!Number.isFinite(maxTicks) || maxTicks < 1) maxTicks = 1;
	const s = span(v);
	let guard = 0;
	if (!dates) {
		const candidates = [1, 2, 3, 6, 12, 24];
		const step = candidates.find((c) => s / c <= maxTicks) ?? 60;
		let m = Math.ceil(v.min / step) * step;
		while (m <= v.max && guard++ < MAX_TICK_LOOPS) {
			out.push({ at: m, label: `M${m}`, major: m % 12 === 0 });
			m += step;
		}
		return out;
	}
	const start = dayToDate(v.min);
	const y0 = start.getUTCFullYear();
	const m0 = start.getUTCMonth();
	if (s <= maxTicks * 1.2) {
		// Tage
		for (let d = Math.ceil(v.min); d <= v.max && guard++ < MAX_TICK_LOOPS; d += 1) {
			const dt = dayToDate(d);
			out.push({
				at: d,
				label: `${pad2(dt.getUTCDate())}.${pad2(dt.getUTCMonth() + 1)}.`,
				major: dt.getUTCDate() === 1
			});
		}
	} else if (s <= maxTicks * 8) {
		// Wochen, Montag
		let d = Math.ceil(v.min);
		while (dayToDate(d).getUTCDay() !== 1 && guard++ < 7) d += 1;
		for (; d <= v.max && guard++ < MAX_TICK_LOOPS; d += 7) {
			const dt = dayToDate(d);
			out.push({
				at: d,
				label: `${pad2(dt.getUTCDate())}.${pad2(dt.getUTCMonth() + 1)}.`,
				major: dt.getUTCDate() <= 7
			});
		}
	} else {
		const monthsTotal = s / 30.4;
		const stepMonths =
			monthsTotal <= maxTicks
				? 1
				: monthsTotal <= maxTicks * 3
					? 3
					: monthsTotal <= maxTicks * 6
						? 6
						: 12;
		let y = y0;
		let m = m0;
		if (stepMonths > 1) m = Math.floor(m / stepMonths) * stepMonths;
		while (guard++ < MAX_TICK_LOOPS) {
			const d = dateToDay(new Date(Date.UTC(y, m, 1)));
			if (d > v.max) break;
			if (d >= v.min) {
				const label =
					stepMonths >= 12
						? `${y}`
						: m === 0 || out.length === 0
							? `${MONTHS[m] ?? ''} ${y}`
							: (MONTHS[m] ?? '');
				out.push({ at: d, label, major: m === 0 });
			}
			m += stepMonths;
			while (m >= 12) {
				m -= 12;
				y += 1;
			}
		}
	}
	return out;
}

/** Lesbares Datum für einen Tageswert. */
export function fmtDay(days: number): string {
	const d = dayToDate(days);
	return `${pad2(d.getUTCDate())}.${pad2(d.getUTCMonth() + 1)}.${d.getUTCFullYear()}`;
}

/** „3 Tage“, „2 Wochen“, „4 Monate“ für eine Spanne in Tagen. */
export function fmtDuration(days: number, dates: boolean): string {
	if (!istZahl(days)) return '–';
	if (!dates) {
		const m = Math.round(days * 10) / 10;
		return `${m.toLocaleString('de-DE')} Monat${m === 1 ? '' : 'e'}`;
	}
	const d = Math.round(days);
	if (d < 14) return `${d} Tag${d === 1 ? '' : 'e'}`;
	if (d < 60) return `${Math.round(d / 7)} Wochen`;
	if (d < 540) return `${Math.round(d / 30.4)} Monate`;
	const y = Math.round((d / 365.25) * 10) / 10;
	return `${y.toLocaleString('de-DE')} Jahre`;
}
