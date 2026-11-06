import { describe, expect, it } from 'vitest';
import type { Datei, FortschrittZeile, Gantt, Kennzahlen, ProjektDaten } from '$vertrag';
import {
	ampel,
	datumZuTag,
	fluss,
	fortschrittZeile,
	gantt,
	ganttFehler,
	kanban,
	karten,
	kennzahlen,
	teile
} from './daten.ts';

const leer: ProjektDaten = {
	kennzahlen: null,
	ampel: null,
	fortschritt: null,
	zeitstrahl: null,
	gantt: null,
	kanban: null,
	fluss: null,
	fragen: null,
	orte: null,
	fristen: null,
	eigen: {}
};

describe('Datei<T>', () => {
	it('liefert ein Ganzes ohne Namen', () => {
		expect(teile({ kennzahl: [] })).toEqual([{ name: null, inhalt: { kennzahl: [] } }]);
	});
	it('liefert Teile in Reihenfolge der Datei, so wie das JSON sie bringt', () => {
		// So schickt der Server `[teil.zukunft]` vor `[teil.anfang]`: umgekehrt zum Alphabet.
		const json =
			'{"teil":{"zukunft":{"kennzahl":[]},"anfang":{"kennzahl":[]},"mitte":{"kennzahl":[]}}}';
		const d = JSON.parse(json) as Datei<Kennzahlen>;
		expect(teile(d).map((x) => x.name)).toEqual(['zukunft', 'anfang', 'mitte']);
	});
});

describe('Übersetzungen', () => {
	it('Kennzahlen mit Einheit und Hinweis', () => {
		expect(
			kennzahlen({
				kennzahl: [
					{ titel: 'Preis', wert: '12', einheit: '€', hinweis: 'brutto', farbe: null },
					{ titel: 'Stück', wert: '3 von 5', einheit: null, hinweis: null, farbe: 'ok' }
				]
			})
		).toEqual([
			{ label: 'Preis', value: '12 €', note: 'brutto' },
			{ label: 'Stück', value: '3 von 5', note: '' }
		]);
	});
	it('Fortschritt aus Prozent oder aus „3 von 5“', () => {
		const zeile = (x: Partial<FortschrittZeile>): FortschrittZeile => ({
			titel: 'A',
			prozent: null,
			erreicht: null,
			gesamt: null,
			hinweis: null,
			...x
		});
		// Etappe 3a+: die Stufe kommt vom Server (`stufe`), die Oberfläche rechnet sie nicht.
		expect(fortschrittZeile(zeile({ prozent: 70, stufe: 'ab-haelfte' }))).toMatchObject({
			percent: 70,
			text: '70 %',
			stufe: 'ab-haelfte'
		});
		expect(fortschrittZeile(zeile({ prozent: 70 })).stufe).toBeNull();
		expect(fortschrittZeile(zeile({ erreicht: 3, gesamt: 5, hinweis: 'h' }))).toMatchObject({
			percent: 60,
			text: '3 von 5',
			note: 'h'
		});
		expect(fortschrittZeile(zeile({ erreicht: 1, gesamt: 4 }))).toMatchObject({
			percent: 25,
			text: '1 von 4'
		});
		expect(fortschrittZeile(zeile({ prozent: 120, stufe: 'erreicht' }))).toMatchObject({
			percent: 100,
			stufe: 'erreicht'
		});
		// Etappe B1: Kommazahl mit Einheit, deutsch geschrieben.
		expect(fortschrittZeile(zeile({ erreicht: 0.25, gesamt: 1.5, einheit: 'GB' }))).toMatchObject({
			text: '0,25 von 1,5 GB'
		});
		expect(fortschrittZeile(zeile({ erreicht: 0.2, gesamt: 1.5 })).percent).toBeCloseTo(13.33, 1);
	});
	it('Etappe B1: Kennzahl und Ampel mit Stand vom Server', () => {
		expect(
			kennzahlen({
				kennzahl: [
					{
						titel: 'Briefe',
						wert: '14',
						einheit: null,
						hinweis: null,
						farbe: null,
						stand: '2026-10-20',
						stand_alter: { tage: 14, stufe: 'alt' }
					}
				]
			})
		).toEqual([
			{
				label: 'Briefe',
				value: '14',
				note: '',
				stand: { stand: '2026-10-20', alter: { tage: 14, stufe: 'alt' } }
			}
		]);
		expect(
			ampel({ zeile: [{ titel: 'Post', farbe: 'ok', hinweis: null, stand: '2026-11-01' }] })
		).toEqual([
			{ bedeutung: 'ok', label: 'Post', note: '', stand: { stand: '2026-11-01', alter: null } }
		]);
	});
	it('Kanban-Karten mit Hinweis, Spalte ohne Farbe ist neutral', () => {
		expect(
			kanban({
				spalte: [
					{
						titel: 'offen',
						farbe: null,
						karte: [
							{ titel: 'Logo', hinweis: 'bis Freitag', farbe: null },
							{ titel: 'Text', hinweis: null, farbe: null }
						]
					}
				]
			})
		).toEqual([{ title: 'offen', bedeutung: 'neutral', items: ['Logo · bis Freitag', 'Text'] }]);
	});
	it('Fluss: Spalten nach längstem Weg, unbekannte Pfeile fallen weg', () => {
		const f = fluss({
			kasten: [
				{ id: 'a', text: 'Aussaat', farbe: null },
				{ id: 'b', text: 'Pflege', farbe: null },
				{ id: 'c', text: 'Erntefest', farbe: null }
			],
			pfeil: [
				{ von: 'a', nach: 'b', text: null },
				{ von: 'b', nach: 'c', text: null },
				{ von: 'a', nach: 'c', text: null },
				{ von: 'a', nach: 'x', text: null }
			]
		});
		expect(f.nodes.map((n) => [n.id, n.layer, n.slot])).toEqual([
			['a', 0, 0],
			['b', 1, 0],
			['c', 2, 0]
		]);
		expect(f.edges).toEqual([
			[0, 1],
			[1, 2],
			[0, 2]
		]);
	});
});

describe('Gantt', () => {
	const balken = (von: number | string, bis: number | string, gruppe: string | null = null) => ({
		name: `${String(von)}-${String(bis)}`,
		von,
		bis,
		farbe: null,
		gruppe
	});

	it('Daten werden Tage wie im Rust-Server', () => {
		const g = gantt({ start: null, balken: [balken('2026-11', '2026-11-10')] });
		if ('fehler' in g) throw new Error(g.fehler);
		expect(g.dates).toBe(true);
		expect(g.bars[0]?.start).toBe(739_921);
		expect(g.bars[0]?.start_iso).toBe('2026-11-01');
		expect(g.bars[0]?.end_iso).toBe('2026-11-10');
		expect(g.bars[0]?.bedeutung).toBe('neutral');
	});
	it('Plan-Monate mit start werden Daten', () => {
		const g = gantt({ start: '2026-11', balken: [balken(0, 2)] });
		if ('fehler' in g) throw new Error(g.fehler);
		expect(g.dates).toBe(true);
		expect(g.bars[0]?.start).toBe(datumZuTag('2026-11'));
		expect(g.bars[0]?.end).toBeCloseTo(739_921 + 60.8);
	});
	it('Plan-Monate ohne start bleiben Monate', () => {
		const g = gantt({ start: null, balken: [balken(0, 6), balken(6, 17)] });
		if ('fehler' in g) throw new Error(g.fehler);
		expect(g.dates).toBe(false);
		expect([g.min, g.max]).toEqual([0, 17]);
		expect(g.bars[1]?.start_iso).toBeNull();
	});
	it('gemischt ist ein Fehler, ohne und mit start (wie im Rust-Server)', () => {
		const satz =
			'Der Balkenplan mischt Plan-Monate und Daten: „0-2“ hat Monatszahlen, „2026-11-2027-01“ hat Daten. Bitte alle Balken gleich, entweder `von = 0` oder `von = "2026-11"`.';
		for (const start of [null, '2026-11']) {
			const g: Gantt = { start, balken: [balken(0, 2), balken('2026-11', '2027-01')] };
			expect(gantt(g)).toEqual({ fehler: satz });
		}
		expect(ganttFehler({ start: null, balken: [balken(0, '2027-01')] })).toBe(
			'Beim Balken „0-2027-01“ sind `von` und `bis` verschieden: bitte beide als Datum oder beide als Monatszahl.'
		);
	});
	it('bis vor von ist ein Fehler, als Zahl und als Datum', () => {
		expect(ganttFehler({ start: null, balken: [balken(0, 2), balken(5, 4.5)] })).toBe(
			'Beim Balken „5-4.5“ liegt `bis` vor `von`.'
		);
		expect(ganttFehler({ start: null, balken: [balken('2027-01', '2026-11-30')] })).toBe(
			'Beim Balken „2027-01-2026-11-30“ liegt `bis` vor `von`.'
		);
		expect(
			ganttFehler({ start: null, balken: [balken(2, 2), balken('2026-11', '2026-11-03')] })
		).toMatch(/mischt/);
		expect(ganttFehler({ start: null, balken: [balken('2026-11', '2026-11-03')] })).toBeNull();
		const g = gantt({ start: null, balken: [balken(3, 1)] });
		expect(g).toEqual({ fehler: 'Beim Balken „3-1“ liegt `bis` vor `von`.' });
	});
	it('hält Gruppen zusammen', () => {
		const g = gantt({
			start: null,
			balken: [balken(0, 1, 'A'), balken(1, 2, 'B'), balken(2, 3, 'A')]
		});
		if ('fehler' in g) throw new Error(g.fehler);
		expect(g.bars.map((b) => b.name)).toEqual(['0-1', '2-3', '1-2']);
	});
});

describe('karten', () => {
	it('nur vorhandene Bausteine, in fester Reihenfolge, Teile benannt', () => {
		const k = karten({
			...leer,
			fragen: {
				frage: [
					{
						id: 'farbe',
						text: 'Welche Farbe?',
						art: 'wahl',
						optionen: ['grün', 'blau'],
						empfehlung: 'grün',
						gestellt: '2026-10-26'
					}
				]
			},
			kennzahlen: { teil: { plan: { kennzahl: [] }, ist: { kennzahl: [] } } }
		});
		expect(k.map((x) => x.titel)).toEqual(['Kennzahlen', 'Fragen']);
		expect(k[0]?.teile.map((t) => t.name)).toEqual(['plan', 'ist']);
		expect(k[1]?.teile[0]?.anzeige.art).toBe('fragen');
	});

	it('Orte (lokal öffnen): eine eigene Karte ohne Teile; den Baustein mit „Öffnen“ setzt NeueForm ein', () => {
		const k = karten({
			...leer,
			orte: { ort: [{ kennung: 'bauplan', titel: 'Bauplan', hinweis: null }] }
		});
		expect(k.map((x) => [x.schluessel, x.titel, x.teile.length])).toEqual([
			['orte', 'Orte auf diesem PC', 0]
		]);
	});
	it('Zeitstrahl: Karte ohne Teile (der Verlauf lädt seine Ereignisse über ladeDaten)', () => {
		const k = karten({ ...leer, zeitstrahl: { eintrag: [] } });
		expect(k).toEqual([{ schluessel: 'zeitstrahl', titel: 'Zeitstrahl', teile: [] }]);
	});
	it('ein Gantt-Fehler wird ein Fehler-Block', () => {
		const k = karten({ ...leer, gantt: { start: null, balken: [] } });
		const a = k[0]?.teile[0]?.anzeige;
		expect(a?.art === 'block' && a.block.kind).toBe('error');
	});
});
