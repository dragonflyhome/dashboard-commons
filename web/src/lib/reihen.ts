/**
 * Farben für Reihen und Anteile (Ordner auf /pc, Konfetti). Nur Token aus
 * app.css (--reihe-1 bis --reihe-10), nie feste Farben.
 */
export const REIHEN: readonly string[] = Array.from(
	{ length: 10 },
	(_, i) => `var(--reihe-${String(i + 1)})`
);

/** Farbe der n-ten Reihe; nach der zehnten beginnt es von vorn. */
export function reihe(n: number): string {
	return REIHEN[((n % REIHEN.length) + REIHEN.length) % REIHEN.length] ?? 'var(--reihe-1)';
}
