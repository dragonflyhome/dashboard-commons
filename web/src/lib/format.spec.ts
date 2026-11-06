import { describe, expect, it } from 'vitest';
import {
	fmtAgo,
	fmtBytes,
	fmtDate,
	fmtDateTime,
	fmtPercent,
	fmtUptime,
	gitStandHinweis,
	istZahl
} from './format';
import { fortschrittZeile } from './daten.ts';
import { formatNumber } from './kpi.ts';
import { fmtDuration } from './timescale.ts';

describe('format', () => {
	it('Abnahme B1: nan, inf und null werden „–“, nie ein Absturz', () => {
		for (const x of [Number.NaN, Infinity, -Infinity, null, undefined, '1']) {
			expect(istZahl(x)).toBe(false);
		}
		expect(istZahl(0)).toBe(true);
		const nichts = null as unknown as number;
		expect(fmtBytes(nichts)).toBe('–');
		expect(fmtBytes(Infinity)).toBe('–');
		expect(fmtPercent(nichts)).toBe('–');
		expect(formatNumber(Number.NaN, 1)).toBe('–');
		expect(fmtDuration(nichts, true)).toBe('–');
		const z = fortschrittZeile({
			titel: 'A',
			prozent: null,
			erreicht: Number.NaN,
			gesamt: 2,
			hinweis: null
		});
		expect(z).toMatchObject({ percent: 0, text: '– von 2' });
	});
	it('bytes', () => {
		expect(fmtBytes(0)).toBe('0 B');
		expect(fmtBytes(1536)).toBe('1,5 KB');
		// ab 100 keine Nachkommastelle mehr
		expect(fmtBytes(526_650_757_120)).toBe('490 GB');
		expect(fmtBytes(1023151951872)).toBe('953 GB');
		expect(fmtBytes(18_366_099_456)).toBe('17,1 GB');
	});
	it('uptime', () => {
		expect(fmtUptime(59)).toBe('0 Min.');
		expect(fmtUptime(3600 * 5 + 120)).toBe('5 Std., 2 Min.');
		expect(fmtUptime(86_400 * 3 + 3600)).toBe('3 Tage, 1 Std.');
	});
	it('dates', () => {
		expect(fmtDate('2026-10-25')).toBe('25.10.2026');
		expect(fmtDate('2026-10')).toBe('10/2026');
		expect(fmtDate('bald')).toBe('bald');
		expect(fmtDateTime(null)).toBe('–');
		expect(fmtDateTime('kaputt')).toBe('kaputt');
		expect(fmtDateTime('2026-10-25T18:16:27+01:00')).toMatch(/25\.10\.2026/);
	});
	it('ago', () => {
		const now = new Date('2026-10-25T12:00:00Z');
		expect(fmtAgo('2026-10-25T11:59:58Z', now)).toBe('gerade eben');
		expect(fmtAgo('2026-10-25T11:57:00Z', now)).toBe('vor 3 Minuten');
		expect(fmtAgo('2026-10-25T09:00:00Z', now)).toBe('vor 3 Stunden');
		expect(fmtAgo(null, now)).toBe('–');
	});
	it('git-stand', () => {
		// Ortszeit, damit die Uhrzeit nicht von der Zeitzone des Rechners abhängt.
		const now = new Date(2026, 9, 27, 14, 30);
		const um = (t: Date) => t.toISOString();
		expect(gitStandHinweis(null, now)).toBe('Git-Stand wird ermittelt');
		expect(gitStandHinweis(um(new Date(2026, 9, 27, 14, 29)), now)).toBeNull();
		expect(gitStandHinweis(um(new Date(2026, 9, 27, 14, 25)), now)).toBeNull();
		expect(gitStandHinweis(um(new Date(2026, 9, 27, 14, 2)), now)).toBe('Git-Stand von 14:02');
		expect(gitStandHinweis(um(new Date(2026, 9, 26, 9, 5)), now)).toBe(
			'Git-Stand von 26.10. 09:05'
		);
		expect(gitStandHinweis('kaputt', now)).toBeNull();
	});
});
