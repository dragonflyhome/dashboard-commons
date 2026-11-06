/** Zahlen und Zeiten lesbar machen, auf Deutsch. */

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const;

/**
 * Eine Zahl, mit der sich rechnen und die sich schreiben lässt (Abnahme B1):
 * TOML kennt `nan` und `inf`, im JSON werden sie `null`. Wer formatiert,
 * prüft vorher hiermit und schreibt sonst „–“.
 */
export function istZahl(n: unknown): n is number {
	return typeof n === 'number' && Number.isFinite(n);
}

export function fmtBytes(bytes: number): string {
	if (!istZahl(bytes)) return '–';
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < UNITS.length - 1) {
		value /= 1024;
		unit += 1;
	}
	const digits = unit === 0 ? 0 : value >= 100 ? 0 : 1;
	return `${value.toLocaleString('de-DE', { maximumFractionDigits: digits, minimumFractionDigits: digits })} ${UNITS[unit] ?? 'B'}`;
}

export function fmtPercent(p: number): string {
	if (!istZahl(p)) return '–';
	return `${Math.round(p).toLocaleString('de-DE')} %`;
}

export function fmtUptime(secs: number): string {
	const d = Math.floor(secs / 86_400);
	const h = Math.floor((secs % 86_400) / 3600);
	const m = Math.floor((secs % 3600) / 60);
	if (d > 0) return `${d} Tag${d === 1 ? '' : 'e'}, ${h} Std.`;
	if (h > 0) return `${h} Std., ${m} Min.`;
	return `${m} Min.`;
}

/** ISO-Zeitstempel → „25.10.2026, 18:16“. Unlesbares bleibt wie es ist. */
export function fmtDateTime(iso: string | null | undefined): string {
	if (iso === null || iso === undefined || iso === '') return '–';
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return iso;
	return d.toLocaleString('de-DE', {
		day: '2-digit',
		month: '2-digit',
		year: 'numeric',
		hour: '2-digit',
		minute: '2-digit'
	});
}

const WOCHENTAGE = ['So', 'Mo', 'Di', 'Mi', 'Do', 'Fr', 'Sa'] as const;

/**
 * Etappe 4c: Tag und Uhrzeit für den Inhaber, „Sa 31.10., 14:05 Uhr“ (Ortszeit);
 * aus einem anderen Jahr mit Jahr („Mi 30.12.2026, 09:00 Uhr“). Leer oder
 * unlesbar: `null`.
 */
export function fmtTagUhrzeit(
	iso: string | null | undefined,
	now: Date = new Date()
): string | null {
	if (iso === null || iso === undefined || iso === '') return null;
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return null;
	const zwei = (n: number): string => String(n).padStart(2, '0');
	const jahr = d.getFullYear() === now.getFullYear() ? '' : String(d.getFullYear());
	const tag = `${WOCHENTAGE[d.getDay()] ?? ''} ${zwei(d.getDate())}.${zwei(d.getMonth() + 1)}.${jahr}`;
	return `${tag}, ${zwei(d.getHours())}:${zwei(d.getMinutes())} Uhr`;
}

/** „2026-10-25“ → „25.10.2026“; „2026-10“ → „10/2026“; sonst unverändert. */
export function fmtDate(text: string): string {
	const full = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text);
	if (full) return `${full[3] ?? ''}.${full[2] ?? ''}.${full[1] ?? ''}`;
	const month = /^(\d{4})-(\d{2})$/.exec(text);
	if (month) return `${month[2] ?? ''}/${month[1] ?? ''}`;
	return text;
}

/** „vor 3 Minuten“, für Zeitstempel in der Nähe von jetzt. */
export function fmtAgo(iso: string | null | undefined, now: Date = new Date()): string {
	if (iso === null || iso === undefined) return '–';
	const t = new Date(iso).getTime();
	if (Number.isNaN(t)) return iso;
	const s = Math.max(0, Math.round((now.getTime() - t) / 1000));
	if (s < 10) return 'gerade eben';
	if (s < 60) return `vor ${s} Sekunden`;
	const m = Math.round(s / 60);
	if (m < 60) return `vor ${m} Minute${m === 1 ? '' : 'n'}`;
	const h = Math.round(m / 60);
	if (h < 48) return `vor ${h} Stunde${h === 1 ? '' : 'n'}`;
	return `vor ${Math.round(h / 24)} Tagen`;
}

/** Ab diesem Alter nennt die Repo-Zeile die Zeit ihres Git-Stands. */
export const GIT_STAND_ALT_MS = 5 * 60 * 1000;

/**
 * Hinweis unter einer Repo-Zeile: „Git-Stand wird ermittelt“ vor der ersten
 * Abfrage, „Git-Stand von 14:02“ (an einem anderen Tag mit Datum), wenn der
 * Stand älter als fünf Minuten ist, sonst nichts.
 */
export function gitStandHinweis(stand: string | null, now: Date = new Date()): string | null {
	if (stand === null) return 'Git-Stand wird ermittelt';
	const t = new Date(stand);
	if (Number.isNaN(t.getTime())) return null;
	if (now.getTime() - t.getTime() <= GIT_STAND_ALT_MS) return null;
	const uhr = t.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' });
	if (t.toDateString() === now.toDateString()) return `Git-Stand von ${uhr}`;
	const tag = t.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit' });
	return `Git-Stand von ${tag} ${uhr}`;
}
