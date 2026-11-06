import { describe, expect, it } from 'vitest';
import { splitNumber } from './kpi';

describe('splitNumber', () => {
	it('findet die Zahl mit Präfix und Einheit', () => {
		expect(splitNumber('24')).toEqual({ prefix: '', number: 24, decimals: 0, suffix: '' });
		expect(splitNumber('42')).toEqual({ prefix: '', number: 42, decimals: 0, suffix: '' });
		expect(splitNumber('~3,5 %')).toEqual({ prefix: '~', number: 3.5, decimals: 1, suffix: ' %' });
		expect(splitNumber('3,5 %')).toEqual({ prefix: '', number: 3.5, decimals: 1, suffix: ' %' });
		expect(splitNumber('1.200 €')).toEqual({ prefix: '', number: 1200, decimals: 0, suffix: ' €' });
		expect(splitNumber('45 kg')).toEqual({ prefix: '', number: 45, decimals: 0, suffix: ' kg' });
		expect(splitNumber('1.250 Stück')).toEqual({
			prefix: '',
			number: 1250,
			decimals: 0,
			suffix: ' Stück'
		});
		expect(splitNumber('-12')).toEqual({ prefix: '', number: -12, decimals: 0, suffix: '' });
		expect(splitNumber('0')).toEqual({ prefix: '', number: 0, decimals: 0, suffix: '' });
		expect(splitNumber('0,5 l')).toEqual({ prefix: '', number: 0.5, decimals: 1, suffix: ' l' });
	});

	it('lässt Text ohne Zahl in Ruhe', () => {
		expect(splitNumber('offen')).toEqual({
			prefix: '',
			number: null,
			decimals: 0,
			suffix: 'offen'
		});
	});

	it.each([
		'01/2025',
		'02–03/2026',
		'06/2025 bis 06/2026',
		'2–3',
		'40–60 Liter',
		'2026-11-02',
		'2.11.2026',
		'Q3 2026',
		'3 von 8',
		'07',
		'-05',
		// Würde am Ende anders geschrieben („1.200“, „3“):
		'1200',
		'3.5',
		'1 200 €',
		// Ein Wort vor der Zahl: Name oder Schätzung, nicht zählen.
		'Baumeister (4b)',
		'Stufe 3',
		'ca. 3,5 %',
		'ca. 2.000',
		'Über 30 %'
	])('zählt „%s“ nicht und lässt den Text unverändert', (text) => {
		expect(splitNumber(text)).toEqual({ prefix: '', number: null, decimals: 0, suffix: text });
	});
});
