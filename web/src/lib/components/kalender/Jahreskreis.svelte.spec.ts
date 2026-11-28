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

	it('der erste Tag gehört zum Festjahr, der Tag nach dem Ende (erster des nächsten) nicht', () => {
		const zeiger = (tag: string) =>
			render(Jahreskreis, { jahr: JAHR, tag }).container.querySelector('path.zeiger');
		expect(zeiger(JAHR.zeitraum.anfang)).not.toBeNull();
		expect(zeiger(JAHR.zeitraum.ende)).toBeNull();
	});

	it('Vollmonde vor dem Anfang des gezeigten Festjahrs bekommen keinen Ring', () => {
		// Das kommende Festjahr mit den nächsten Vollmonden ab einem Tag im laufenden.
		const kommend = { ...JAHR, zeitraum: { anfang: '2027-11-28', ende: '2028-12-16' } };
		const { container } = render(Jahreskreis, {
			jahr: kommend,
			tag: '2027-06-21',
			textOffen: true
		});
		expect(container.querySelectorAll('circle.naechster')).toHaveLength(0);
		const zeilen = [...container.querySelectorAll('[data-textfassung] tbody tr')].map(
			(z) => z.textContent
		);
		expect(zeilen.filter((z) => z.includes('vor diesem Festjahr'))).toHaveLength(3);
		expect(zeilen.some((z) => z.includes('Ring im Bild'))).toBe(false);
	});
});

/** Winkel eines Punkts um die Mitte (200 200) in Grad, im Uhrzeigersinn ab oben, 0 bis 360. */
function winkelVon(x: number, y: number): number {
	return ((Math.atan2(x - 200, 200 - y) * 180) / Math.PI + 360) % 360;
}
/** Die Zahlen eines Pfads `M x y L x y` oder `M x y A … x y`: Anfang und Ende. */
function enden(d: string | null | undefined): [number, number, number, number] {
	const z = (d ?? '').match(/-?\d+(?:\.\d+)?/g)?.map(Number) ?? [];
	return [z[0] ?? NaN, z[1] ?? NaN, z.at(-2) ?? NaN, z.at(-1) ?? NaN];
}
/** Die Winkel der Marken der Sonnentage, nach ihrer Rolle. */
function marken(jahr: KalenderJahr, tag = jahr.zeitraum.anfang): Record<string, number> {
	const { container } = render(Jahreskreis, { jahr, tag });
	const pfade = [...container.querySelectorAll('path.marke')];
	return Object.fromEntries(
		jahr.sonnentage.map((s, i) => {
			const [x, y] = enden(pfade[i]?.getAttribute('d'));
			return [s.kennung, winkelVon(x, y)];
		})
	);
}
/** Wie weit zwei Winkel auseinanderliegen, über die Null hinweg gerechnet. */
function abstand(a: number, b: number): number {
	return Math.abs(((a - b + 540) % 360) - 180);
}
/** Ein Festjahr aus Neumonden (der letzte ist das Ende) und den vier Sonnentagen nach Rolle. */
function festjahr(neumonde: string[], sonne: Record<string, string>): KalenderJahr {
	const anfaenge = neumonde.slice(0, -1);
	return {
		...JAHR,
		zeitraum: { anfang: neumonde[0] ?? '', ende: neumonde.at(-1) ?? '' },
		monate: anfaenge.map((anfang, i) => ({
			nummer: i + 1,
			name: `Mond ${String(i + 1)}`,
			namen: [],
			jahreszeit: null,
			zeitraum: { anfang, vollmond: anfang, ende: neumonde[i + 1] ?? '' }
		})),
		sonnentage: Object.entries(sonne).map(([kennung, datum]) => ({
			kennung,
			name: kennung,
			datum,
			zeit: '12:00'
		})),
		naechste_vollmonde: []
	};
}
/** Ein Festjahr mit 13 Monden, 384 Tage. */
const DREIZEHN = festjahr(
	[
		'2027-11-28',
		'2027-12-27',
		'2028-01-26',
		'2028-02-25',
		'2028-03-26',
		'2028-04-24',
		'2028-05-24',
		'2028-06-22',
		'2028-07-22',
		'2028-08-20',
		'2028-09-18',
		'2028-10-18',
		'2028-11-16',
		'2028-12-16'
	],
	{
		'wende-winter': '2027-12-22',
		'gleiche-fruehling': '2028-03-20',
		'wende-sommer': '2028-06-20',
		'gleiche-herbst': '2028-09-22'
	}
);
/** Sydney: Der kürzeste Tag liegt im Juni, die Rollen sind dieselben. */
const SYDNEY = festjahr(
	[
		'2027-06-05',
		'2027-07-04',
		'2027-08-02',
		'2027-09-01',
		'2027-09-30',
		'2027-10-29',
		'2027-11-28',
		'2027-12-27',
		'2028-01-26',
		'2028-02-25',
		'2028-03-26',
		'2028-04-24',
		'2028-05-24'
	],
	{
		'gleiche-herbst': '2028-03-20',
		'wende-sommer': '2027-12-22',
		'gleiche-fruehling': '2027-09-23',
		'wende-winter': '2027-06-21'
	}
);
const KREUZ = {
	'wende-winter': 0,
	'gleiche-fruehling': 90,
	'wende-sommer': 180,
	'gleiche-herbst': 270
};

describe('Jahreskreis: Sonnentage auf dem Kreuz', () => {
	it.each([
		['mit 12 Monden', JAHR],
		['mit 13 Monden', DREIZEHN],
		['in Sydney, nach der Rolle und nicht nach der Reihenfolge der Liste', SYDNEY]
	])('die vier Marken stehen bei 0°, 90°, 180° und 270° (%s)', (_, jahr) => {
		const w = marken(jahr);
		for (const [rolle, soll] of Object.entries(KREUZ)) {
			expect(abstand(w[rolle] ?? NaN, soll), rolle).toBeLessThan(0.5);
		}
	});

	it.each([
		['12 Monde', JAHR],
		['13 Monde', DREIZEHN]
	])('Anfang und Ende des Festjahrs treffen sich, der Kreis schließt (%s)', (_, jahr) => {
		const { container } = render(Jahreskreis, { jahr, tag: jahr.zeitraum.anfang });
		const baender = [...container.querySelectorAll('path.monat')];
		const [x1, y1] = enden(baender[0]?.getAttribute('d'));
		const [, , x2, y2] = enden(baender.at(-1)?.getAttribute('d'));
		// Jedes Band ist an beiden Enden um 1,2° gekürzt: Die Lücke misst 2,4°.
		expect(abstand(winkelVon(x1, y1), winkelVon(x2, y2) + 2.4)).toBeLessThan(0.3);
	});

	it('der Zeiger liegt an einem Sonnentag genau auf dessen Marke', () => {
		const tag = '2027-06-21';
		const { container } = render(Jahreskreis, { jahr: JAHR, tag });
		const i = JAHR.sonnentage.findIndex((s) => s.datum === tag);
		const [mx, my] = enden(container.querySelectorAll('path.marke')[i]?.getAttribute('d'));
		const [, , zx, zy] = enden(container.querySelector('path.zeiger')?.getAttribute('d'));
		expect(abstand(winkelVon(zx, zy), winkelVon(mx, my))).toBeLessThan(0.1);
		expect(abstand(winkelVon(zx, zy), 180)).toBeLessThan(0.1);
	});

	it('ohne Sonnentage gilt die gleichmäßige Rechnung über das Festjahr, ohne den Satz in der Legende', () => {
		const ohne = { ...JAHR, sonnentage: [] };
		const { container } = render(Jahreskreis, { jahr: ohne, tag: '2027-06-08' });
		expect(container.querySelectorAll('path.marke')).toHaveLength(0);
		// Ohne Wende steht der Anfang des Festjahrs oben; die Mitte des Jahrs (Tag 181 von 354) fast unten.
		const [, , zx, zy] = enden(container.querySelector('path.zeiger')?.getAttribute('d'));
		expect(abstand(winkelVon(zx, zy), (181 / 354) * 360)).toBeLessThan(0.1);
		expect(container.querySelector('[data-legende]')?.textContent).not.toMatch(/Kreuz/);
	});

	it('fehlt ein Sonnentag, gilt für alle Marken die gleichmäßige Rechnung', () => {
		const drei = {
			...JAHR,
			sonnentage: JAHR.sonnentage.filter((s) => s.kennung !== 'gleiche-herbst')
		};
		// 182 Tage nach der Wende von 354: 185,1° wie vor der Berichtigung.
		expect(abstand(marken(drei)['wende-sommer'] ?? NaN, (182 / 354) * 360)).toBeLessThan(0.1);
	});

	it('die Legende sagt, dass die Monde vor der Wintersonnenwende gedehnt oder gestaucht sind', () => {
		const { container } = render(Jahreskreis, { jahr: JAHR, tag: '2027-06-21' });
		expect(container.querySelector('[data-legende]')?.textContent).toMatch(
			/Die Sonnentage stehen auf dem Kreuz; die Monde im Viertel vor der Wintersonnenwende sind\s+gedehnt oder gestaucht/
		);
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
