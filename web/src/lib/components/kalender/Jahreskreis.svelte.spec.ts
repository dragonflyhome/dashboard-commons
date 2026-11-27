import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { KalenderJahr } from '$vertrag';
import { JAHR } from '#lib/textfassung.beispiele.ts';
import Jahreskreis from './Jahreskreis.svelte';

/** Die Flächen der Jahreszeiten als (Klasse, Wort, von, bis). */
function flaechen(jahr: KalenderJahr): [string, string, string, string][] {
	const { container } = render(Jahreskreis, { jahr, tag: '2027-06-21' });
	return [...container.querySelectorAll('path.zeit')].map((p) => [
		[...p.classList].filter((k) => !k.startsWith('svelte-')).join(' '),
		p.nextElementSibling?.textContent ?? '',
		p.getAttribute('data-von') ?? '',
		p.getAttribute('data-bis') ?? ''
	]);
}

describe('Jahreskreis: Ausrichtung', () => {
	it('die Wintersonnenwende steht oben (Winkel null), nicht der Anfang des ersten Monats', () => {
		const { container } = render(Jahreskreis, { jahr: JAHR, tag: '2026-12-21' });
		const i = JAHR.sonnentage.findIndex((s) => s.kennung === 'wende-winter');
		const marke = container.querySelectorAll('path.marke')[i];
		// Senkrecht über der Mitte (200 200): von Halbmesser 104 bis 122.
		expect(marke?.getAttribute('d')).toBe('M 200.0 96.0 L 200.0 78.0');
		// Am Tag der Wende zeigt der Zeiger senkrecht nach oben.
		expect(container.querySelector('path.zeiger')?.getAttribute('d')).toMatch(
			/^M 200\.0 164\.0 L 200\.0 /
		);
		// Der erste Monat beginnt am 9.12., also links vor der Spitze.
		const start = Number(container.querySelector('path.monat')?.getAttribute('d')?.split(' ')[1]);
		expect(start).toBeLessThan(200);
	});

	it('zeigt keinen Zeiger und kein „am …“, wenn das gezeigte Festjahr den Tag nicht enthält', () => {
		const { container } = render(Jahreskreis, { jahr: JAHR, tag: '2028-01-15' });
		expect(container.querySelector('path.zeiger')).toBeNull();
		expect(container.querySelector('svg')?.getAttribute('aria-label')).not.toMatch(/ am /);
	});
});

describe('Jahreskreis: Jahreszeiten', () => {
	it('beginnen mit dem Neumond nach ihrem Festmonat, nicht am Sonnentag; der Winter über den Jahresanfang ist eine Fläche', () => {
		expect(flaechen(JAHR)).toEqual([
			['zeit winter', 'Winter', '2027-10-29', '2027-02-06'],
			['zeit fruehling', 'Frühling', '2027-02-06', '2027-05-06'],
			['zeit sommer', 'Sommer', '2027-05-06', '2027-08-02'],
			['zeit herbst', 'Herbst', '2027-08-02', '2027-10-29']
		]);
	});

	it('Endet das Festjahr mit Samhain, steht der Herbst zuletzt und der Winter beginnt erst im nächsten', () => {
		const ohneLetzten = { ...JAHR, monate: JAHR.monate.slice(0, -1) };
		const f = flaechen(ohneLetzten);
		expect(f.map((x) => x[1])).toEqual(['Winter', 'Frühling', 'Sommer', 'Herbst']);
		expect(f[0]?.[2]).toBe('2026-12-09');
		expect(f[3]?.[3]).toBe('2027-10-29');
	});

	it('ohne Monate (zwischen den Wendekreisen) keine Flächen', () => {
		expect(flaechen({ ...JAHR, monate: [] })).toEqual([]);
	});
});
