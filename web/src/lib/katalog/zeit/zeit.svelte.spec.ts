import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { SvelteSet } from 'svelte/reactivity';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GanttData, VerlaufEreignis, VerlaufQuelle } from '$vertrag';
import Verlauf from './Verlauf.svelte';
import Dichtestreifen from './Dichtestreifen.svelte';
import Gantt from './Gantt.svelte';
import Zeitachse from './Zeitachse.svelte';
import { tagZahl } from '#lib/zeit/tage.ts';
import { PUFFER_MS } from '#lib/tageswechsel.ts';

/** Freitag, 30.10.2026, mittags. */
const JETZT = new Date(2026, 9, 30, 12, 0);

/** Ein Element, das da sein muss. */
function el(x: Element | null | undefined): HTMLElement {
	if (!(x instanceof HTMLElement)) throw new Error('Element fehlt');
	return x;
}

let nr = 0;
function e(
	tag: number,
	h: number,
	mehr: Partial<VerlaufEreignis> = {},
	monat = 10
): VerlaufEreignis {
	nr += 1;
	return {
		kennung: `k:${String(nr)}`,
		zeit: new Date(2026, monat - 1, tag, h, 0).toISOString(),
		genauigkeit: 'minute',
		bis: null,
		titel: `Eintrag ${String(nr)}`,
		text: null,
		quelle: 'hand',
		art: 'erledigt',
		verweis: null,
		...mehr
	};
}

const liste: VerlaufEreignis[] = [
	e(30, 9, { titel: 'Netzteil getauscht', art: 'erledigt' }),
	e(30, 8, { quelle: 'update', art: 'routine', titel: 'Defender 1' }),
	e(30, 7, { quelle: 'update', art: 'routine', titel: 'Defender 2' }),
	e(30, 6, { quelle: 'update', art: 'routine', titel: 'Defender 3' }),
	e(30, 5, { quelle: 'update', art: 'routine', titel: 'Defender 4' }),
	e(29, 10, {
		titel: 'Gestern passiert',
		quelle: 'meldung',
		art: 'problem',
		verweis: '/anstehend'
	}),
	e(27, 10, { titel: 'Dienstag passiert', text: 'x'.repeat(200) }),
	e(5, 10, { titel: 'Anfang Oktober' })
];

describe('Verlauf', () => {
	it('zeigt Überschriften je Tag, Heute und Gestern offen, Routine in einer Zeile', async () => {
		render(Verlauf, {
			ereignisse: liste,
			jetzt: JETZT,
			hinweise: ['Ohne Anmeldung fehlen die Starts.']
		});
		const heute = screen.getByRole('button', { name: /Heute/ });
		expect(heute).toHaveAttribute('aria-expanded', 'true');
		expect(screen.getByRole('heading', { name: /Gestern/ })).toBeInTheDocument();
		expect(screen.getByText('Netzteil getauscht')).toBeInTheDocument();
		const routine = screen.getByRole('button', { name: /4 Updates installiert/ });
		expect(screen.queryByText('Defender 1')).not.toBeInTheDocument();
		await fireEvent.click(routine);
		expect(screen.getByText('Defender 1')).toBeInTheDocument();
		// Dienstag ist zu, mit Anzahl.
		const dienstag = screen.getByRole('button', { name: /Dienstag, 27\.10\..*1 Eintrag/ });
		expect(dienstag).toHaveAttribute('aria-expanded', 'false');
		expect(screen.queryByText('Dienstag passiert')).not.toBeInTheDocument();
		await fireEvent.click(dienstag);
		expect(screen.getByText('Dienstag passiert')).toBeInTheDocument();
		// Lange Texte: zwei Zeilen, dann „mehr“.
		expect(screen.getByRole('button', { name: 'mehr' })).toBeInTheDocument();
		// Quelle klein rechts, Farbe nach art; Verweis als Link.
		expect(screen.getByRole('link', { name: 'Gestern passiert' })).toHaveAttribute(
			'href',
			'/anstehend'
		);
		expect(screen.getByText('Problem:', { exact: false })).toBeInTheDocument();
		expect(screen.getByText(/1 Hinweis zur Vollständigkeit/)).toBeInTheDocument();
	});

	it('Filter nach Quelle mit Anzahl', async () => {
		render(Verlauf, { ereignisse: liste, jetzt: JETZT });
		const filter = screen.getByRole('group', { name: 'Nach Quelle filtern' });
		const updates = within(filter).getByRole('button', { name: /Updates 4/ });
		await fireEvent.click(updates);
		expect(updates).toHaveAttribute('aria-pressed', 'false');
		expect(screen.queryByRole('button', { name: /Updates installiert/ })).not.toBeInTheDocument();
	});

	it('Tastatur: Pfeile von Eintrag zu Eintrag, Pos1 auf heute', async () => {
		const { container } = render(Verlauf, { ereignisse: liste, jetzt: JETZT });
		const ziele = [...container.querySelectorAll<HTMLElement>('[data-verlauf-ziel]')];
		// jsdom hat kein Layout: alle Ziele gelten als unsichtbar; das Springen prüft die Edge-Probe.
		expect(ziele.length).toBeGreaterThan(3);
		ziele[2]?.focus();
		await fireEvent.keyDown(el(ziele[2]), { key: 'Home' });
		expect(document.activeElement?.closest('[data-heute]')).not.toBeNull();
	});

	it('Projektseiten: Kommendes über der Heute-Linie, darunter je Jahr', () => {
		const { container } = render(Verlauf, {
			modus: 'projekt',
			jetzt: JETZT,
			ereignisse: [
				e(6, 0, { titel: 'Termin', art: 'geplant' }, 11),
				e(1, 0, { titel: 'Früher', genauigkeit: 'tag', zeit: '2025-03-01T00:00:00+01:00' })
			]
		});
		const texte = [...container.querySelectorAll('h3, .heute-linie, .titel')].map((x) =>
			x.textContent.trim()
		);
		expect(texte).toEqual(['Kommt noch', 'Termin', 'Heute, 30.10.2026', '2025', 'Früher']);
	});

	it('Enter klappt nur auf, wenn der Eintrag selbst den Fokus hat; ein Link darin bleibt Link', async () => {
		const lang = e(30, 9, { titel: 'Lang mit Link', text: 'y'.repeat(200), verweis: '/vorgaenge' });
		const { container } = render(Verlauf, { ereignisse: [lang], jetzt: JETZT });
		const link = screen.getByRole('link', { name: 'Lang mit Link' });
		link.focus();
		// fireEvent liefert true, wenn niemand preventDefault gerufen hat: der Link folgt.
		expect(await fireEvent.keyDown(link, { key: 'Enter', cancelable: true })).toBe(true);
		expect(screen.getByRole('button', { name: 'mehr' })).toBeInTheDocument();
		const eintrag = el(container.querySelector('[data-kennung]'));
		eintrag.focus();
		expect(await fireEvent.keyDown(eintrag, { key: 'Enter', cancelable: true })).toBe(false);
		expect(screen.getByRole('button', { name: 'weniger' })).toBeInTheDocument();
	});

	it('ohne Einträge steht der Satz aus leer, auch auf der PC-Seite', () => {
		render(Verlauf, {
			ereignisse: [],
			jetzt: JETZT,
			leer: 'In den letzten Tagen ist nichts passiert.'
		});
		expect(screen.getByText('In den letzten Tagen ist nichts passiert.')).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: /Heute/ })).not.toBeInTheDocument();
	});

	it('zeigt nur den Zeitraum und lässt ihn aufheben', async () => {
		let weg = 0;
		render(Verlauf, {
			ereignisse: liste,
			jetzt: JETZT,
			zeitraum: { von: '2026-10-27', bis: '2026-10-27' },
			onzeitraumweg: () => (weg += 1)
		});
		expect(screen.getByText('Dienstag passiert')).toBeInTheDocument();
		expect(screen.queryByText('Netzteil getauscht')).not.toBeInTheDocument();
		await fireEvent.click(screen.getByRole('button', { name: 'alle Tage zeigen' }));
		expect(weg).toBe(1);
	});
});

describe('Dichtestreifen', () => {
	it('91 Balken, Tastatur: Pfeile, + und -, Pos1; Enter springt', async () => {
		const gewaehlt: string[] = [];
		render(Dichtestreifen, {
			ereignisse: liste,
			jetzt: JETZT,
			onwaehle: (t: string) => gewaehlt.push(t)
		});
		const s = screen.getByRole('slider');
		expect(s).toHaveAttribute('data-cursor', '2026-10-30');
		expect(s).toHaveAttribute('aria-valuetext', 'Heute: 5 Einträge');
		expect(s.querySelectorAll('[data-tag]')).toHaveLength(91);
		await fireEvent.keyDown(s, { key: '+' });
		expect(s.querySelectorAll('[data-tag]')).toHaveLength(30);
		await fireEvent.keyDown(s, { key: 'ArrowLeft' });
		await fireEvent.keyDown(s, { key: 'ArrowLeft' });
		await fireEvent.keyDown(s, { key: 'ArrowLeft' });
		expect(s).toHaveAttribute('data-cursor', '2026-10-27');
		expect(s).toHaveAttribute('aria-valuetext', 'Dienstag, 27.10.: 1 Eintrag');
		await fireEvent.keyDown(s, { key: 'Enter' });
		expect(gewaehlt).toEqual(['2026-10-27']);
		// Alt+Pfeil bleibt beim Browser.
		await fireEvent.keyDown(s, { key: 'ArrowLeft', altKey: true });
		expect(s).toHaveAttribute('data-cursor', '2026-10-27');
		await fireEvent.keyDown(s, { key: 'Home' });
		expect(s).toHaveAttribute('data-cursor', '2026-10-30');
		await fireEvent.keyDown(s, { key: '-' });
		expect(s.querySelectorAll('[data-tag]')).toHaveLength(91);
	});
});

describe('Dichtestreifen (Auflagen aus 3a-1)', () => {
	it('der älteste der 91 Tage ist da; Ende geht hin, Bild ab und Bild auf je eine Woche', async () => {
		render(Dichtestreifen, {
			ereignisse: [e(1, 10, { titel: 'ältester Tag' }, 8)],
			jetzt: JETZT
		});
		const s = screen.getByRole('slider');
		expect(s.querySelector('[data-tag]')).toHaveAttribute('data-tag', '2026-08-01');
		expect(s).toHaveAttribute('aria-valuemin', String(tagZahl('2026-08-01')));
		await fireEvent.keyDown(s, { key: 'End' });
		expect(s).toHaveAttribute('data-cursor', '2026-08-01');
		expect(s).toHaveAttribute('aria-valuetext', 'Samstag, 01.08.: 1 Eintrag');
		await fireEvent.keyDown(s, { key: 'PageDown' });
		expect(s).toHaveAttribute('data-cursor', '2026-08-01');
		await fireEvent.keyDown(s, { key: 'PageUp' });
		expect(s).toHaveAttribute('data-cursor', '2026-08-08');
		await fireEvent.keyDown(s, { key: 'Home' });
		await fireEvent.keyDown(s, { key: 'PageDown' });
		expect(s).toHaveAttribute('data-cursor', '2026-10-23');
		await fireEvent.keyDown(s, { key: 'Home' });
		await fireEvent.keyDown(s, { key: 'PageUp' });
		expect(s).toHaveAttribute('data-cursor', '2026-10-30');
	});

	it('der ARIA-Text nennt den gezoomten Ausschnitt', async () => {
		render(Dichtestreifen, { ereignisse: liste, jetzt: JETZT });
		const s = screen.getByRole('slider');
		expect(s.getAttribute('aria-label')).toMatch(
			/^Einträge je Tag, die letzten 91 Tage bis heute\./
		);
		expect(s.getAttribute('aria-label')).toMatch(/Bild auf und Bild ab eine Woche.*Ende/);
		await fireEvent.keyDown(s, { key: '+' });
		expect(s.getAttribute('aria-label')).toMatch(
			/^Einträge je Tag, die letzten 30 Tage bis heute\./
		);
		await fireEvent.keyDown(s, { key: 'End' });
		expect(s).toHaveAttribute('data-von', '2026-08-01');
		expect(s.getAttribute('aria-label')).toMatch(
			/^Einträge je Tag, 30 Tage vom 01\.08\.2026 bis 30\.08\.2026\./
		);
	});

	it('folgt dem Quellenfilter des Verlaufs (dieselbe Menge aus)', async () => {
		const aus = new SvelteSet<VerlaufQuelle>();
		render(Verlauf, { ereignisse: liste, jetzt: JETZT, aus });
		render(Dichtestreifen, { ereignisse: liste, jetzt: JETZT, aus });
		const s = screen.getByRole('slider');
		expect(s).toHaveAttribute('aria-valuetext', 'Heute: 5 Einträge');
		const filter = screen.getByRole('group', { name: 'Nach Quelle filtern' });
		await fireEvent.click(within(filter).getByRole('button', { name: /Updates 4/ }));
		expect(aus.has('update')).toBe(true);
		expect(s).toHaveAttribute('aria-valuetext', 'Heute: 1 Eintrag');
		await fireEvent.click(within(filter).getByRole('button', { name: /Updates 4/ }));
		expect(s).toHaveAttribute('aria-valuetext', 'Heute: 5 Einträge');
	});
});

describe('Verlauf (Auflagen aus 3a-1)', () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	it('Sprung auf einen leeren Tag: genau ein Leer-Satz', async () => {
		// Gar nichts da: nur der Satz aus leer, nicht noch der Satz zum Tag.
		const leer = render(Verlauf, {
			ereignisse: [],
			jetzt: JETZT,
			sprung: { tag: '2026-10-28', nr: 1 }
		});
		await vi.waitFor(() => {
			expect(screen.getByText('Noch keine Einträge.')).toBeInTheDocument();
		});
		expect(screen.queryByText(/An diesem Tag steht nichts/)).not.toBeInTheDocument();
		leer.unmount();
		// Sprung auf heute, Heute ist leer: nur „Noch nichts.“ in der Gruppe.
		const heute = render(Verlauf, {
			ereignisse: [e(29, 10)],
			jetzt: JETZT,
			sprung: { tag: '2026-10-30', nr: 1 }
		});
		await vi.waitFor(() => {
			expect(screen.getByText('Noch nichts.')).toBeInTheDocument();
		});
		expect(screen.queryByText(/An diesem Tag steht nichts/)).not.toBeInTheDocument();
		heute.unmount();
		// Ein anderer leerer Tag: nur der Satz zum Tag.
		render(Verlauf, { ereignisse: liste, jetzt: JETZT, sprung: { tag: '2026-10-28', nr: 1 } });
		await vi.waitFor(() => {
			expect(screen.getAllByText(/An diesem Tag steht nichts/)).toHaveLength(1);
		});
		expect(screen.queryByText('Noch nichts.')).not.toBeInTheDocument();
	});

	it('am Monatsersten trägt nur Heute data-heute; Pos1 landet dort', async () => {
		const { container } = render(Verlauf, {
			ereignisse: [
				e(1, 9, { titel: 'im November', genauigkeit: 'monat', zeit: '2026-11-01T00:00:00+01:00' }),
				e(31, 9, { titel: 'Samstag' })
			],
			jetzt: new Date(2026, 10, 1, 12, 0)
		});
		const mitHeute = [...container.querySelectorAll('[data-heute]')];
		expect(mitHeute.map((x) => x.getAttribute('data-gruppe'))).toEqual(['tag:2026-11-01']);
		const samstag = el(container.querySelector('[data-gruppe="tag:2026-10-31"] button'));
		samstag.focus();
		await fireEvent.keyDown(samstag, { key: 'Home' });
		expect(document.activeElement?.closest('[data-heute]')).toBe(mitHeute[0]);
	});

	it('über Mitternacht offen: Heute rechnet neu (falsche Zeitgeber)', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		vi.setSystemTime(new Date(2026, 9, 30, 23, 59, 0));
		const { container } = render(Verlauf, {
			ereignisse: [e(30, 9, { titel: 'Netzteil getauscht' })]
		});
		render(Dichtestreifen, { ereignisse: [] });
		const heute = (): string | null | undefined =>
			container.querySelector('[data-heute]')?.getAttribute('data-gruppe');
		expect(heute()).toBe('tag:2026-10-30');
		expect(screen.getByRole('slider')).toHaveAttribute('data-cursor', '2026-10-30');
		await vi.advanceTimersByTimeAsync(30_000);
		flushSync();
		expect(heute()).toBe('tag:2026-10-30');
		await vi.advanceTimersByTimeAsync(30_000 + PUFFER_MS);
		flushSync();
		expect(heute()).toBe('tag:2026-10-31');
		expect(screen.getByRole('heading', { name: /Gestern/ })).toBeInTheDocument();
		expect(screen.getByRole('slider')).toHaveAttribute('data-cursor', '2026-10-31');
	});
});

describe('Zeitachse', () => {
	it('Beschriftungen je Stufe, ohne Überlappung, mit Heute-Linie', () => {
		const { container } = render(Zeitachse, {
			fenster: { von: tagZahl('2026-10-26'), bis: tagZahl('2026-11-02') },
			breite: 480,
			stufe: 'tag',
			heute: tagZahl('2026-10-30') + 0.5
		});
		const texte = [...container.querySelectorAll('[data-beschriftung]')].map((x) => x.textContent);
		expect(texte).toEqual(['Mo 26.', 'Di 27.', 'Mi 28.', 'Do 29.', 'Fr 30.', 'Sa 31.', '1. Nov.']);
		expect(container.querySelector('[data-heute-linie]')).not.toBeNull();
	});
});

const nov1 = 739_921;
const plan: GanttData = {
	dates: true,
	min: nov1,
	max: nov1 + 400,
	bars: [
		{
			name: 'Phase 0',
			start: nov1,
			end: nov1 + 30,
			start_iso: '2026-11-01',
			end_iso: '2026-12-01',
			bedeutung: 'ok'
		},
		{
			name: 'Phase 1',
			start: nov1 + 30,
			end: nov1 + 400,
			start_iso: '2026-12-01',
			end_iso: null,
			bedeutung: 'wartet'
		}
	],
	ticks: []
};

describe('Gantt', () => {
	it('Namen und Zeiträume, Filter nach Status', async () => {
		render(Gantt, { data: plan, jetzt: JETZT });
		expect(screen.getByText('Phase 0')).toBeInTheDocument();
		expect(screen.getByText(/01\.11\.2026 bis 01\.12\.2026/)).toBeInTheDocument();
		await fireEvent.click(screen.getByRole('button', { name: /wartet/ }));
		expect(screen.queryByText('Phase 1')).not.toBeInTheDocument();
	});

	it('Zoom mit + und -, Verschieben mit Pfeilen, kein Alt+Pfeil; Mausrad nur mit Strg', async () => {
		const { container } = render(Gantt, { data: plan, jetzt: JETZT });
		const g = el(container.querySelector('[data-zeit="gantt"]'));
		const b = screen.getByRole('application');
		expect(g).toHaveAttribute('data-stufe', 'alles');
		await fireEvent.keyDown(b, { key: '+' });
		expect(g.dataset.stufe).not.toBe('alles');
		const von = Number(g.dataset.von);
		await fireEvent.keyDown(b, { key: 'ArrowRight' });
		expect(Number(g.dataset.von)).toBeGreaterThan(von);
		const nachRechts = Number(g.dataset.von);
		await fireEvent.keyDown(b, { key: 'ArrowLeft', altKey: true });
		expect(Number(g.dataset.von)).toBe(nachRechts);
		await fireEvent.keyDown(b, { key: '0' });
		expect(g).toHaveAttribute('data-stufe', 'alles');
		expect(await fireEvent.wheel(b, { deltaY: -100, cancelable: true })).toBe(true);
		expect(await fireEvent.wheel(b, { deltaY: -100, ctrlKey: true, cancelable: true })).toBe(false);
		expect(g.dataset.stufe).not.toBe('alles');
	});

	it('lässt sich auch bei einem Plan von wenigen Tagen vergrößern und verschieben', async () => {
		const kurz: GanttData = {
			...plan,
			max: nov1 + 4,
			bars: [
				{
					name: 'Kurz',
					start: nov1,
					end: nov1 + 4,
					start_iso: '2026-11-01',
					end_iso: '2026-11-05',
					bedeutung: 'ok'
				}
			]
		};
		const { container } = render(Gantt, { data: kurz, jetzt: JETZT });
		const g = el(container.querySelector('[data-zeit="gantt"]'));
		const b = screen.getByRole('application');
		await fireEvent.keyDown(b, { key: '+' });
		expect(g).toHaveAttribute('data-stufe', 'tag');
		const von = Number(g.dataset.von);
		await fireEvent.keyDown(b, { key: 'ArrowRight' });
		expect(Number(g.dataset.von)).toBeGreaterThan(von);
	});

	it('Plan-Monate ohne Datum', () => {
		const m: GanttData = {
			dates: false,
			min: 0,
			max: 12,
			bars: [{ name: 'A', start: 0, end: 6, start_iso: null, end_iso: null, bedeutung: 'neutral' }],
			ticks: []
		};
		const { container } = render(Gantt, { data: m, jetzt: JETZT });
		expect(screen.getByText(/Monat 0 bis Monat 6/)).toBeInTheDocument();
		expect(container.querySelector('[data-zeit="gantt"]')).toHaveAttribute(
			'data-heute-ausserhalb',
			'ja'
		);
	});
});
