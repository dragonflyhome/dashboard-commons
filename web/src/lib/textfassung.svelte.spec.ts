/**
 * Etappe 3b, Textfassung (Ergänzung B der Architektur): jeder Bild-Baustein
 * hat eine Textfassung, und sie zeigt dieselben Zahlen und Daten wie das
 * Bild, für dieselbe Eingabe.
 *
 * 1. Liste: Jede Datei unter lib/katalog und lib/components, die ein Bild
 *    zeichnet (SVG, role=img, role=progressbar, role=meter, Breite oder Höhe
 *    aus Daten), steht in BILD_BAUSTEINE oder mit Grund in KEINE_BILDER.
 * 2. Ort: `selbst` zeigt `<Textfassung` in der eigenen Datei; `aufrufer`
 *    verlangt es in jeder Datei, die den Baustein einbindet.
 * 3. Zahlen: je Baustein Bild und Text aus denselben Beispielen
 *    (textfassung.beispiele.ts) gerendert, die Zahlen verglichen.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { render, screen, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { BILD_BAUSTEINE, FASSUNGEN, KEINE_BILDER, dichteText, ganttText } from './textfassung.ts';
import * as B from './textfassung.beispiele.ts';
import Textfassung from './components/Textfassung.svelte';
import Gantt from './katalog/zeit/Gantt.svelte';
import Dichtestreifen from './katalog/zeit/Dichtestreifen.svelte';
import Ring from './components/pc/Ring.svelte';
import Sparkline from './components/pc/Sparkline.svelte';
import Progress from './components/blocks/Progress.svelte';
import Kpi from './components/blocks/Kpi.svelte';
import Flow from './components/blocks/Flow.svelte';
import PcKurve from './components/daten/Verlauf.svelte';
import Verlauf from './katalog/zeit/Verlauf.svelte';
import { tagText } from './zeit/tage.ts';

const SRC = join(import.meta.dirname, '..');
const BILD = /<svg\b|role="img"|role="progressbar"|role="meter"|style:(?:width|height)=/;

function dateien(pfad: string): string[] {
	if (statSync(pfad).isFile()) return [pfad];
	return readdirSync(pfad).flatMap((n) => dateien(join(pfad, n)));
}
const rel = (p: string): string => relative(SRC, p).split('\\').join('/');
const lesen = (p: string): string => readFileSync(join(SRC, p), 'utf8');

/** Alle Quellen der Oberfläche ohne Tests. */
const QUELLEN = dateien(SRC)
	.filter((p) => /\.(svelte|ts)$/.test(p) && !/\.(spec|test)\.ts$/.test(p))
	.map(rel);

/** Alle Zahlen eines Texts (Datum 31.10.2026 gibt 31, 10, 2026; Komma zählt als Teil der Zahl). */
function zahlen(text: string): string[] {
	return [...text.matchAll(/\d+(?:,\d+)?/g)].map((m) => m[0]);
}

/** Zeilen der Textfassung als Text je Zelle. */
function zeilen(wurzel: HTMLElement): string[][] {
	const tabelle = wurzel.querySelector('[data-textfassung] table');
	if (tabelle === null) throw new Error('keine Textfassung');
	return [...tabelle.querySelectorAll('tbody tr')].map((tr) =>
		[...tr.querySelectorAll('th, td')].map((z) => z.textContent.trim())
	);
}

afterEach(() => {
	vi.restoreAllMocks();
});

describe('Liste der Bild-Bausteine (Etappe 3b)', () => {
	const kandidaten = QUELLEN.filter(
		(p) =>
			(p.startsWith('lib/katalog/') || p.startsWith('lib/components/')) && p.endsWith('.svelte')
	).filter((p) => BILD.test(lesen(p)));

	it('jede Datei mit Bild steht in BILD_BAUSTEINE oder mit Grund in KEINE_BILDER', () => {
		const fehlt = kandidaten.filter((p) => !(p in BILD_BAUSTEINE) && !(p in KEINE_BILDER));
		expect(fehlt, 'Textfassung fehlt (#lib/textfassung.ts)').toEqual([]);
		expect(kandidaten.length).toBeGreaterThan(8);
	});

	it('die Listen sind aktuell: jede Datei gibt es, zeichnet wirklich und hat einen Grund', () => {
		// Bild-Bausteine ohne eigenes Bild-Muster (Kennzahlen zählen hoch, Verlauf ist
		// Liste) dürfen in der Liste stehen; es muss sie nur geben.
		for (const p of Object.keys(BILD_BAUSTEINE)) expect(QUELLEN, p).toContain(p);
		for (const p of Object.keys(KEINE_BILDER)) expect(kandidaten, p).toContain(p);
		for (const grund of Object.values(KEINE_BILDER)) expect(grund.length).toBeGreaterThan(10);
	});

	it.each(Object.entries(BILD_BAUSTEINE))('%s: Textfassung am richtigen Ort', (pfad, b) => {
		if (b.ort === 'eingebaut') {
			expect(b.grund?.length ?? 0).toBeGreaterThan(10);
			return;
		}
		const f = b.funktion;
		if (f === undefined) throw new Error(`${pfad}: Funktion fehlt`);
		expect(FASSUNGEN[f]).toBeTypeOf('function');
		if (b.ort === 'selbst') {
			const text = lesen(pfad);
			expect(text).toContain('<Textfassung');
			expect(text).toContain(`${f}(`);
			return;
		}
		// aufrufer: jede Datei, die den Baustein einbindet, zeigt die Textfassung.
		const name = pfad.slice(pfad.lastIndexOf('/', pfad.lastIndexOf('/') - 1) + 1);
		const einbinder = QUELLEN.filter(
			(q) => q !== pfad && new RegExp(`from '[^']*${name}'`).test(lesen(q))
		);
		expect(einbinder.length, `${pfad} wird nirgends eingebunden`).toBeGreaterThan(0);
		for (const q of einbinder) {
			const text = lesen(q);
			expect(text, q).toContain('<Textfassung');
			expect(text, q).toContain(`${f}(`);
		}
	});
});

describe('Bild und Text zeigen dieselben Zahlen', () => {
	it('Gantt: je Balken Von, Bis und Dauer wie die Beschriftung des Balkens', () => {
		const bild = render(Gantt, { data: B.GANTT, jetzt: new Date(2026, 9, 31, 12) });
		const imBild = [...bild.container.querySelectorAll('.reihe .unsichtbar')].map((s) =>
			zahlen(s.textContent)
		);
		bild.unmount();
		const text = render(Textfassung, { fassung: ganttText(B.GANTT) });
		const z = zeilen(text.container);
		expect(z.map((r) => r[0])).toEqual(B.GANTT.bars.map((b) => b.name));
		expect(z.map((r) => zahlen(`${r[1] ?? ''} ${r[2] ?? ''} ${r[3] ?? ''}`))).toEqual(imBild);
		expect(imBild).toHaveLength(3);
	});

	it('Dichtestreifen: je Tag mit Einträgen dieselbe Zahl wie die Säule', () => {
		const jetzt = new Date(2026, 9, 31, 12);
		const ereignisse = B.dichteEreignisse(jetzt);
		const bild = render(Dichtestreifen, { ereignisse, jetzt });
		const imBild = [...bild.container.querySelectorAll<HTMLElement>('[data-tag]')]
			.filter((t) => t.querySelector('.saeule') !== null)
			.map((t) => t.title)
			.sort();
		bild.unmount();
		const text = render(Textfassung, { fassung: dichteText(ereignisse, tagText(jetzt)) });
		const imText = zeilen(text.container)
			.map(
				([tag, anzahl]) =>
					`${tag ?? ''}: ${anzahl === '1' ? '1 Eintrag' : `${anzahl ?? ''} Einträge`}`
			)
			.sort();
		expect(imText).toEqual(imBild);
		expect(imBild).toHaveLength(4);
	});

	it('Ring: Anteil und Wert wie im Bild', () => {
		const { container } = render(Ring, { ...B.RING });
		const bild = container.querySelector('[role="img"]')?.getAttribute('aria-label') ?? '';
		const [zeile] = zeilen(container);
		expect(zahlen((zeile ?? []).slice(0, 3).join(' '))).toEqual(zahlen(bild));
		expect(zahlen(bild)).toEqual(['72', '23', '32']);
		expect(zeile?.[3]).toBe('hoch');
	});

	it('Kurve: jetzt und Spitze wie im Bild, so viele Messpunkte wie Punkte der Linie', () => {
		const { container } = render(Sparkline, { ...B.KURVE });
		// Nach dem Namen („Prozessor, letzte 15 Minuten:“) stehen die Werte.
		const bild = (container.querySelector('[role="img"]')?.getAttribute('aria-label') ?? '')
			.split(':')
			.slice(1)
			.join(':');
		const text: Record<string, string | undefined> = Object.fromEntries(
			zeilen(container).map((r): [string, string] => [r[0] ?? '', r[1] ?? ''])
		);
		expect([text.jetzt, text.Spitze].map((t) => zahlen(t ?? '')[0])).toEqual(zahlen(bild));
		const punkte = (container.querySelector('path.line')?.getAttribute('d') ?? '').match(/[ML]/g);
		expect(String(punkte?.length)).toBe(text.Messpunkte);
		expect(text.Tiefst).toBe('9 %');
		expect(text.Mittel).toBe('22 %');
	});

	it('Fortschritt: Prozent je Balken wie aria-valuenow, Stand wie der Text darunter', () => {
		const { container } = render(Progress, { rows: B.FORTSCHRITT });
		const balken = screen.getAllByRole('progressbar');
		const z = zeilen(container);
		expect(z.map((r) => zahlen(r[1] ?? '')[0])).toEqual(
			balken.map((b) => b.getAttribute('aria-valuenow'))
		);
		expect(z.map((r) => r[2])).toEqual(B.FORTSCHRITT.map((r) => r.text));
	});

	it('Kennzahlen: Endwert je Kachel wie im Bild (ohne Hochzählen)', () => {
		vi.spyOn(window, 'matchMedia').mockImplementation(
			(q: string) => ({ matches: q.includes('reduce') }) as MediaQueryList
		);
		const { container } = render(Kpi, { items: B.KENNZAHLEN });
		const bild = [...container.querySelectorAll('.kpi .value')].map((v) => v.textContent.trim());
		const z = zeilen(container);
		expect(z.map((r) => zahlen(r[1] ?? ''))).toEqual(bild.map(zahlen));
		expect(z.map((r) => r[0])).toEqual(B.KENNZAHLEN.map((k) => k.label));
	});

	it('Fluss: dieselben Kästen, so viele Pfeile wie im Bild', () => {
		const { container } = render(Flow, { data: B.FLUSS });
		const svg = container.querySelector('svg');
		if (svg === null) throw new Error('kein Bild');
		const kaesten = [...svg.querySelectorAll('g.node')].map((g) => g.getAttribute('aria-label'));
		const pfeile = svg.querySelectorAll('path.edge').length;
		const z = zeilen(container);
		expect(z.map((r) => r[0])).toEqual(kaesten);
		const imText = z
			.map((r) => r[1] ?? '')
			.filter((w) => w !== '–')
			.flatMap((w) => w.split(', '));
		expect(imText).toHaveLength(pfeile);
		expect(pfeile).toBe(3);
	});

	it('PC-Kurve (/daten): Mittel je Reihe wie in der Beschreibung des Bilds', () => {
		const { container } = render(PcKurve, { werte: B.PC_KURVE });
		const bild = within(container).getByRole('img').getAttribute('aria-label') ?? '';
		const mittelBild = [...bild.matchAll(/im Mittel (\d+) Prozent/g)].map((m) => m[1]);
		const z = zeilen(container);
		expect(z.map((r) => zahlen(r[1] ?? '')[0])).toEqual(mittelBild);
		// Lücke (cpu null) zählt nicht als 0.
		expect(z.map((r) => r[4])).toEqual(['3', '4']);
	});

	it('Verlauf ist selbst Text: je Ereignis eine Zeile mit Titel', () => {
		const jetzt = new Date(2026, 9, 31, 12);
		const ereignisse = B.dichteEreignisse(jetzt).filter((e) => e.art !== 'routine');
		const { container } = render(Verlauf, { ereignisse, jetzt, modus: 'projekt' });
		for (const e of ereignisse) expect(container.textContent).toContain(e.titel);
		expect(container.querySelectorAll('li').length).toBeGreaterThanOrEqual(ereignisse.length);
	});
});

describe('Textfassung.svelte', () => {
	it('für Vorlese-Programme immer da, sichtbar über „Als Text“ (gedrückt)', async () => {
		const { container } = render(Textfassung, { fassung: ganttText(B.GANTT) });
		const knopf = screen.getByRole('button', { name: 'Als Text' });
		expect(knopf).toHaveAttribute('aria-pressed', 'false');
		const tabelle = screen.getByRole('table', { name: 'Balkenplan als Tabelle' });
		expect(tabelle.parentElement).toHaveClass('nur-vorlesen');
		knopf.click();
		await Promise.resolve();
		await vi.waitFor(() => {
			expect(knopf).toHaveAttribute('aria-pressed', 'true');
		});
		expect(tabelle.parentElement).not.toHaveClass('nur-vorlesen');
		expect(container.querySelectorAll('tbody tr')).toHaveLength(3);
	});

	it('ohne Zeilen: der Satz statt einer leeren Tabelle', () => {
		render(Textfassung, { fassung: ganttText({ ...B.GANTT, bars: [] }) });
		expect(screen.getByText('Der Plan hat keine Balken.')).toBeInTheDocument();
	});
});
