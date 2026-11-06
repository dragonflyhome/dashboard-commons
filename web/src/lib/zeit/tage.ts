/**
 * Kalendertage als Text `JJJJ-MM-TT` (Ortszeit) und als Tageszahl (Tage seit
 * dem 1.1.1970). Gerechnet wird über die Datumsteile in UTC, damit eine
 * Zeitumstellung keinen Tag verschluckt.
 */

export const MS_TAG = 86_400_000;

const WOCHENTAGE = [
	'Sonntag',
	'Montag',
	'Dienstag',
	'Mittwoch',
	'Donnerstag',
	'Freitag',
	'Samstag'
];
const WOCHENTAGE_KURZ = ['So', 'Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa'];
export const MONATE = [
	'Januar',
	'Februar',
	'März',
	'April',
	'Mai',
	'Juni',
	'Juli',
	'August',
	'September',
	'Oktober',
	'November',
	'Dezember'
];
export const MONATE_KURZ = [
	'Jan.',
	'Feb.',
	'März',
	'Apr.',
	'Mai',
	'Juni',
	'Juli',
	'Aug.',
	'Sept.',
	'Okt.',
	'Nov.',
	'Dez.'
];

function zwei(n: number): string {
	return n < 10 ? `0${String(n)}` : String(n);
}

/** Der Tag eines Zeitpunkts in Ortszeit. */
export function tagText(d: Date): string {
	return `${String(d.getFullYear())}-${zwei(d.getMonth() + 1)}-${zwei(d.getDate())}`;
}

/** Tageszahl eines Tages (`JJJJ-MM-TT`); `NaN`, wenn es keiner ist. */
export function tagZahl(tag: string): number {
	const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(tag);
	if (!m) return Number.NaN;
	return Math.round(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3])) / MS_TAG);
}

/** Der Tag zu einer Tageszahl (ganze Zahl). */
export function tagAusZahl(zahl: number): string {
	const d = new Date(Math.floor(zahl) * MS_TAG);
	return `${String(d.getUTCFullYear())}-${zwei(d.getUTCMonth() + 1)}-${zwei(d.getUTCDate())}`;
}

export function tagPlus(tag: string, n: number): string {
	return tagAusZahl(tagZahl(tag) + n);
}

/** 0 = Sonntag … 6 = Samstag. */
export function wochentag(tag: string): number {
	// Der 1.1.1970 war ein Donnerstag.
	return (((tagZahl(tag) + 4) % 7) + 7) % 7;
}

/** Der Montag der Woche, in der `tag` liegt. */
export function wochenanfang(tag: string): string {
	return tagPlus(tag, -((wochentag(tag) + 6) % 7));
}

export function jahrVon(tag: string): number {
	return Number(tag.slice(0, 4));
}

/** 1 bis 12. */
export function monatVon(tag: string): number {
	return Number(tag.slice(5, 7));
}

/** „26.10.“, mit `jahr` „26.10.2026“. */
export function kurzDatum(tag: string, jahr = false): string {
	const t = `${tag.slice(8, 10)}.${tag.slice(5, 7)}.`;
	return jahr ? `${t}${tag.slice(0, 4)}` : t;
}

/** „Montag, 26.10.“; liegt der Tag in einem anderen Jahr als `heute`, mit Jahr. */
export function tagTitel(tag: string, heute: string): string {
	const name = WOCHENTAGE[wochentag(tag)] ?? '';
	return `${name}, ${kurzDatum(tag, jahrVon(tag) !== jahrVon(heute))}`;
}

/** „Mo 26.“ für Achsen. */
export function tagKurz(tag: string): string {
	return `${WOCHENTAGE_KURZ[wochentag(tag)] ?? ''} ${String(Number(tag.slice(8, 10)))}.`;
}

/** „Oktober 2026“. */
export function monatTitel(tag: string): string {
	return `${MONATE[monatVon(tag) - 1] ?? ''} ${tag.slice(0, 4)}`;
}
