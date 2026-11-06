/**
 * Zerlegt einen Kennzahl-Text wie „3,5 %“ oder „45 kg“ in Präfix, Zahl,
 * Nachkommastellen und Rest, damit `KpiValue` die Zahl hochzählen kann.
 *
 * Gezählt wird nur, wenn der Text genau eine Zahl enthält (davor höchstens
 * Zeichen ohne Buchstaben wie „~“ oder „+“, danach höchstens Wörter oder eine
 * Einheit ohne Ziffern), die Zahl ohne führende Null geschrieben ist und das
 * Dashboard sie am Ende wieder genau so schreibt („1.200“, nicht „1200“).
 * Sonst ist `number` `null` und der Text bleibt, wie er ist: „01/2025“, „2–3“,
 * „06/2025 bis 06/2026“, „2026-11-02“. Steht ein Wort vor der Zahl
 * („Baumeister (4b)“, „ca. 2.000“), gehört die Zahl zu einem Namen oder einer
 * Schätzung; Hochzählen zeigte dort „Baumeister (0b)“.
 */
import { istZahl } from './format.ts';
export interface NumberParts {
	prefix: string;
	number: number | null;
	decimals: number;
	suffix: string;
}

/** Die Zahl so, wie `KpiValue` sie schreibt (deutsch, feste Nachkommastellen). */
export function formatNumber(n: number, decimals: number): string {
	if (!istZahl(n)) return '–';
	return n.toLocaleString('de-DE', {
		minimumFractionDigits: decimals,
		maximumFractionDigits: decimals
	});
}

export function splitNumber(text: string): NumberParts {
	const nichts: NumberParts = { prefix: '', number: null, decimals: 0, suffix: text };
	// Präfix und Rest ohne Ziffern; dazwischen eine Zahl mit Trennzeichen.
	const m = /^([^\d]*?)(-?\d+(?:[.,\s]\d+)*)([^\d]*)$/su.exec(text);
	if (!m) return nichts;
	// Ein Wort (ein Buchstabe) vor der Zahl: nicht zählen.
	if (/\p{L}/u.test(m[1] ?? '')) return nichts;
	const raw = m[2] ?? '';
	// Führende Null („01“, „-05“) heißt: Monat, Nummer oder Datum, nicht zählen.
	if (/^-?0\d/.test(raw)) return nichts;
	// Deutsche Schreibweise: Punkt als Tausender, Komma als Dezimaltrenner.
	const normalised = raw.includes(',')
		? raw.replace(/[.\s]/g, '').replace(',', '.')
		: /^-?\d{1,3}(\.\d{3})+$/.test(raw)
			? raw.replace(/\./g, '')
			: raw;
	const n = Number(normalised);
	if (!Number.isFinite(n)) return nichts;
	const decimals = raw.includes(',') ? (raw.split(',')[1]?.length ?? 0) : 0;
	// Am Ende muss genau der Text stehen, den der Chat geschrieben hat.
	if (formatNumber(n, decimals) !== raw) return nichts;
	return { prefix: m[1] ?? '', number: n, decimals, suffix: m[3] ?? '' };
}
