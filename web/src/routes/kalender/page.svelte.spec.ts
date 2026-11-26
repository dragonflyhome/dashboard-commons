/**
 * Seite /kalender: Tageszeiten, Mond, Feste und „Anderer Ort“. Die
 * Koordinaten eines anderen Orts gehen nur im Rumpf eines POST, nie in die
 * Adresse.
 */
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type {
	KalenderAbfrageAntwort,
	KalenderFeste,
	KalenderGebete,
	KalenderOrt,
	KalenderOrte,
	KalenderStaedte,
	KalenderTag,
	KalenderZonen
} from '$vertrag';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: Page } = await import('./+page.svelte');

const ORTE: KalenderOrte = {
	orte: [
		{ kennung: 'zuhause', titel: 'Zuhause', zeitart: 'Europe/Berlin', heimat: true },
		{ kennung: 'hütte', titel: 'Hütte', zeitart: 'Europe/Oslo', heimat: false }
	]
};

const TAG: KalenderTag = {
	datum: '2026-06-21',
	ort: 'Zuhause',
	zeitart: 'Europe/Berlin',
	zeiten: [
		{
			kennung: 'aufgang',
			name: 'Sonnenaufgang',
			zeit: '04:43',
			text: null,
			herkunft: 'Refraktion 0,833° · Ort Zuhause · Europe/Berlin'
		},
		{
			kennung: 'untergang',
			name: 'Sonnenuntergang',
			zeit: null,
			text: 'Sonne geht nicht unter',
			herkunft: 'Polartag'
		}
	],
	taglaenge: '16 h 50 min',
	laengster_tag: 'Längster Tag · 16 h 50 min',
	mond: {
		kalender: 'Jahreskreis',
		nummer: 7,
		name: 'Rosenmond',
		namen: [],
		zeitraum: { anfang: '2026-06-15', vollmond: '2026-06-29', ende: '2026-07-14' },
		phase: 'zunehmend',
		hinweis: 'Namen sind nicht überall belegt.'
	},
	hinweis: 'Die Zeiten gelten für die Mitte des Orts.',
	gebete: []
};

const FESTE: KalenderFeste = {
	von: '2026-06-21',
	bis: '2026-07-21',
	ort: 'zuhause',
	zeitart: 'Europe/Berlin',
	feste: [
		{
			kennung: 'jahreskreis.litha',
			datum: '2026-06-21',
			titel: 'Litha',
			kalender: 'Jahreskreis',
			herkunft: 'Sommersonnenwende',
			zeitraum: null
		}
	],
	hinweis: null
};

const ANTWORT: KalenderAbfrageAntwort = {
	breite: 52.5,
	laenge: 13.4,
	naeherung: true,
	tag: { ...TAG, ort: null, hinweis: null },
	feste: FESTE
};

const ZONEN: KalenderZonen = { zonen: ['America/Sao_Paulo', 'Europe/Berlin'], tzdb: '2025b' };

const NEU: KalenderOrt = {
	kennung: 'ort-1',
	titel: 'Hafen',
	zeitart: 'Europe/Berlin',
	heimat: false
};

function json(body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status: 200,
		headers: { 'content-type': 'application/json' }
	});
}

// Erfundene Stadt mit erfundenen Koordinaten.
const STAEDTE: KalenderStaedte = {
	treffer: [
		{
			name: 'Nebelfurt',
			land: 'ZZ',
			region: 'Nordmark',
			breite: 12.5,
			laenge: -33.75,
			zeitzone: 'America/Sao_Paulo'
		}
	],
	quelle: 'GeoNames (geonames.org), CC BY 4.0',
	hinweis: null
};

// Erfundene Auswahl: am Heimatort eine Variante, dazu ein Ort, den es nicht mehr gibt.
const GEBETE: KalenderGebete = {
	varianten: [
		{ kennung: 'fuenf-mwl', titel: 'Fünf Zeiten, MWL' },
		{ kennung: 'drei-isna', titel: 'Drei Zeiten, ISNA' }
	],
	auswahl: [
		{ ort: 'zuhause', varianten: ['fuenf-mwl'], fehlt: false },
		{ ort: 'alt', varianten: ['drei-isna'], fehlt: true }
	],
	hinweis: null
};

const TAG_GEBETE: KalenderTag = {
	...TAG,
	gebete: [
		{
			kennung: 'fuenf-mwl',
			titel: 'Fünf Zeiten, MWL',
			zeilen: [
				{
					name: 'Ischa',
					zeit: '22:41',
					text: null,
					umfasst: [],
					hinweis: 'nach der Siebtel-Regel',
					herkunft: 'MWL 17° · Siebtel-Regel · Ort Zuhause · Europe/Berlin'
				}
			],
			hinweis: null
		},
		{
			kennung: 'drei-isna',
			titel: 'Drei Zeiten, ISNA',
			zeilen: [
				{
					name: 'Abend und Nacht',
					zeit: null,
					text: 'keine Zeit',
					umfasst: ['Ischa'],
					hinweis: null,
					herkunft: 'ISNA · Refraktion 0,833° · Ort Zuhause · Europe/Berlin'
				}
			],
			hinweis: 'An diesem Tag fehlen hier Zeiten.'
		}
	]
};

/** Ein gefälschtes fetch, das je Pfad antwortet und jeden Aufruf mitschreibt. */
function server(
	tag: KalenderTag = TAG,
	staedte: KalenderStaedte = STAEDTE
): ReturnType<typeof vi.fn> {
	let orte = ORTE;
	const f = vi.fn((pfad: string, init?: RequestInit) => {
		if (pfad === '/api/kalender/gebete') return Promise.resolve(json(GEBETE));
		if (pfad === '/api/kalender/staedte' && init?.method === 'POST')
			return Promise.resolve(json(staedte));
		if (pfad === '/api/kalender/standorte' && init?.method === 'POST') {
			orte = { orte: [...ORTE.orte, NEU] };
			return Promise.resolve(json(NEU));
		}
		if (pfad.startsWith('/api/kalender/standorte')) return Promise.resolve(json(orte));
		if (pfad.startsWith('/api/kalender/zonen')) return Promise.resolve(json(ZONEN));
		if (pfad.startsWith('/api/kalender/tag')) return Promise.resolve(json(tag));
		if (pfad.startsWith('/api/kalender/feste')) return Promise.resolve(json(FESTE));
		if (pfad.startsWith('/api/kalender/abfrage') && init?.method === 'POST')
			return Promise.resolve(json(ANTWORT));
		return Promise.resolve(new Response('{}', { status: 404 }));
	});
	vi.stubGlobal('fetch', f);
	return f;
}

afterEach(() => {
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
});

describe('Seite /kalender', () => {
	it('zeigt Tageszeiten mit Herkunft, den Satz statt der Zeit, Hinweise, Mond und Feste', async () => {
		const f = server();
		render(Page);
		expect(await screen.findByText('04:43')).toBeInTheDocument();
		expect(screen.getByText('Sonnenaufgang')).toBeInTheDocument();
		expect(screen.getByText('Sonne geht nicht unter')).toBeInTheDocument();
		expect(screen.getByText('Refraktion 0,833° · Ort Zuhause · Europe/Berlin')).toBeInTheDocument();
		expect(screen.getByText('Die Zeiten gelten für die Mitte des Orts.')).toBeInTheDocument();
		expect(screen.getByText(/Uhrzeiten in Europe\/Berlin/)).toBeInTheDocument();
		expect(screen.getByText('Taglänge: 16 h 50 min')).toBeInTheDocument();
		expect(screen.getByText('Längster Tag · 16 h 50 min')).toBeInTheDocument();
		expect(screen.getByText(/Rosenmond/)).toBeInTheDocument();
		expect(screen.getByText('Phase: zunehmend')).toBeInTheDocument();
		expect(screen.getByText('Namen sind nicht überall belegt.')).toBeInTheDocument();
		expect(screen.getByText('Litha')).toBeInTheDocument();
		expect(screen.getByText('Jahreskreis · Sommersonnenwende')).toBeInTheDocument();
		// Der Heimatort (vom Server zuerst geliefert) ist vorgewählt.
		const pfade = f.mock.calls.map((c) => String(c[0]));
		expect(pfade.some((p) => p.startsWith('/api/kalender/tag?') && p.includes('ort=zuhause'))).toBe(
			true
		);
	});

	it('„Anderer Ort“ schickt die Koordinaten im Rumpf eines POST, nie in der Adresse', async () => {
		const f = server();
		render(Page);
		await screen.findByText('04:43');
		await fireEvent.input(screen.getByLabelText(/Breite/), { target: { value: '52.52' } });
		await fireEvent.input(screen.getByLabelText(/Länge/), { target: { value: '13.405' } });
		const form = screen.getByRole('group', { name: 'Anderer Ort' }).closest('form');
		if (form === null) throw new Error('Formular fehlt');
		await fireEvent.submit(form);
		expect(
			await screen.findByText(/Gerechnet für Breite 52,5° und Länge 13,4°/)
		).toBeInTheDocument();
		expect(screen.getByText(/aus der Länge geschätzt/)).toBeInTheDocument();

		const post = f.mock.calls.find((c) => String(c[0]).startsWith('/api/kalender/abfrage'));
		if (post === undefined) throw new Error('kein POST');
		const [pfad, init] = post as [string, RequestInit];
		expect(init.method).toBe('POST');
		const rumpf = JSON.parse(init.body as string) as Record<string, unknown>;
		expect(rumpf).toMatchObject({ breite: 52.52, laenge: 13.405, zeitzone: null });
		for (const [p] of f.mock.calls as [string][]) {
			expect(p).not.toMatch(/52\.52|13\.405|breite|laenge/);
		}
		expect(pfad).toBe('/api/kalender/abfrage');
		expect(localStorage.length).toBe(0);
		expect(sessionStorage.length).toBe(0);
	});

	it('Mond zwischen den Wendekreisen: ohne Namen, mit Hinweis, kein Absturz', async () => {
		const mond = TAG.mond;
		if (mond === null) throw new Error('Mond fehlt');
		server({ ...TAG, mond: { ...mond, namen: null, nummer: null, phase: 'Vollmond' } });
		render(Page);
		expect(await screen.findByText('Phase: Vollmond')).toBeInTheDocument();
		expect(screen.getByText(/Rosenmond/)).toBeInTheDocument();
	});

	it('Zeitzonen kommen aus /api/kalender/zonen, die Fassung steht klein dabei', async () => {
		const f = server();
		const { container } = render(Page);
		expect(await screen.findByText('Zeitzonen-Daten 2025b')).toBeInTheDocument();
		expect(f.mock.calls.some((c) => String(c[0]) === '/api/kalender/zonen')).toBe(true);
		const werte = [...container.querySelectorAll('datalist#kalender-zonen option')].map((o) =>
			o.getAttribute('value')
		);
		expect(werte).toEqual(ZONEN.zonen);
		expect(screen.getByLabelText(/Zeitzone/)).toHaveAttribute('list', 'kalender-zonen');
	});

	it('„Ort merken“ speichert erst auf Klick, Koordinaten nur im Rumpf, danach ist der Ort gewählt', async () => {
		const f = server();
		render(Page);
		await screen.findByText('04:43');
		expect(screen.queryByRole('button', { name: 'Ort merken' })).toBeNull();
		await fireEvent.input(screen.getByLabelText(/Breite/), { target: { value: '52.52' } });
		await fireEvent.input(screen.getByLabelText(/Länge/), { target: { value: '13.405' } });
		const form = screen.getByRole('group', { name: 'Anderer Ort' }).closest('form');
		if (form === null) throw new Error('Formular fehlt');
		await fireEvent.submit(form);
		await screen.findByText(/Gerechnet für Breite/);
		const gespeichert = () =>
			f.mock.calls.filter(
				(c) =>
					String(c[0]) === '/api/kalender/standorte' &&
					(c[1] as RequestInit | undefined)?.method === 'POST'
			);
		expect(gespeichert()).toHaveLength(0);

		await fireEvent.input(screen.getByLabelText(/Name für diesen Ort/), {
			target: { value: ' Hafen ' }
		});
		const knopf = screen.getByRole('button', { name: 'Ort merken' }).closest('form');
		if (knopf === null) throw new Error('Merken-Formular fehlt');
		await fireEvent.submit(knopf);
		const gemerkt = await screen.findByText('Gemerkt als „Hafen“.');
		expect(gemerkt).toHaveAttribute('role', 'status');
		await vi.waitFor(() => {
			expect(document.activeElement).toBe(gemerkt);
		});

		expect(gespeichert()).toHaveLength(1);
		const rumpf = JSON.parse((gespeichert()[0]?.[1] as RequestInit).body as string) as Record<
			string,
			unknown
		>;
		expect(rumpf).toEqual({ titel: 'Hafen', breite: 52.52, laenge: 13.405, zeitzone: null });
		for (const [p] of f.mock.calls as [string][]) {
			expect(p).not.toMatch(/52.52|13.405|breite|laenge/);
		}
		// Danach rechnet die Seite für den neuen Ort über seine Kennung.
		await vi.waitFor(() => {
			const pfade = f.mock.calls.map((c) => String(c[0]));
			expect(pfade.some((p) => p.startsWith('/api/kalender/tag?') && p.includes('ort=ort-1'))).toBe(
				true
			);
		});
		expect(localStorage.length).toBe(0);
		expect(sessionStorage.length).toBe(0);
	});

	it('„Stadt suchen“ schickt den Suchtext im Rumpf, zeigt die Quelle und füllt Breite, Länge und Zone', async () => {
		const f = server();
		render(Page);
		await screen.findByText('04:43');
		// Die Namensnennung steht dauerhaft am Suchfeld (Lizenzpflicht), auch vor der Suche.
		expect(screen.getByText('Städteliste: GeoNames (geonames.org), CC BY 4.0')).toBeInTheDocument();
		await fireEvent.input(screen.getByLabelText('Stadt suchen'), {
			target: { value: '  Nebelf  ' }
		});
		await fireEvent.keyDown(screen.getByLabelText('Stadt suchen'), { key: 'Enter' });
		const stadt = await screen.findByRole('button', { name: 'Nebelfurt, Nordmark (ZZ)' });
		expect(screen.getByText('Städteliste: GeoNames (geonames.org), CC BY 4.0')).toBeInTheDocument();

		const post = f.mock.calls.find((c) => String(c[0]) === '/api/kalender/staedte');
		if (post === undefined) throw new Error('kein POST');
		const init = post[1] as RequestInit;
		expect(init.method).toBe('POST');
		expect(JSON.parse(init.body as string)).toEqual({ anfrage: 'Nebelf', hoechstens: 10 });
		for (const [p] of f.mock.calls as [string][]) expect(p).not.toMatch(/Nebelf/);
		// Enter im Suchfeld schickt „Anderer Ort“ nicht ab.
		expect(f.mock.calls.some((c) => String(c[0]).startsWith('/api/kalender/abfrage'))).toBe(false);

		await fireEvent.click(stadt);
		expect(screen.getByLabelText(/Breite/)).toHaveValue(12.5);
		expect(screen.getByLabelText(/Länge/)).toHaveValue(-33.75);
		expect(screen.getByLabelText(/Zeitzone/)).toHaveValue('America/Sao_Paulo');
		expect(document.activeElement).toBe(screen.getByLabelText(/Breite/));
		expect(screen.getByText(/Städteliste:/)).toBeInTheDocument();
		expect(localStorage.length).toBe(0);
		expect(sessionStorage.length).toBe(0);
	});

	it('Gebetszeiten: Varianten nebeneinander mit Hinweis und Herkunft; ein Schalter speichert am benannten Ort', async () => {
		const f = server(TAG_GEBETE);
		render(Page);
		expect(await screen.findByText('22:41')).toBeInTheDocument();
		expect(screen.getByRole('region', { name: 'Fünf Zeiten, MWL' })).toBeInTheDocument();
		expect(screen.getByRole('region', { name: 'Drei Zeiten, ISNA' })).toBeInTheDocument();
		expect(screen.getByText('nach der Siebtel-Regel')).toBeInTheDocument();
		expect(
			screen.getByText('MWL 17° · Siebtel-Regel · Ort Zuhause · Europe/Berlin')
		).toBeInTheDocument();
		expect(screen.getByText('keine Zeit')).toBeInTheDocument();
		expect(screen.getByText('mit Ischa')).toBeInTheDocument();
		expect(screen.getByText('An diesem Tag fehlen hier Zeiten.')).toBeInTheDocument();
		expect(screen.getByText(/Den Ort „alt“ gibt es nicht mehr/)).toBeInTheDocument();
		expect(screen.getByRole('switch', { name: 'Fünf Zeiten, MWL' })).toHaveAttribute(
			'aria-checked',
			'true'
		);
		const drei = screen.getByRole('switch', { name: 'Drei Zeiten, ISNA' });
		expect(drei).toHaveAttribute('aria-checked', 'false');

		const gespeichert = () =>
			f.mock.calls
				.filter(
					(c) =>
						String(c[0]) === '/api/kalender/gebete' &&
						(c[1] as RequestInit | undefined)?.method === 'POST'
				)
				.map((c) => JSON.parse((c[1] as RequestInit).body as string) as unknown);
		await fireEvent.click(drei);
		await vi.waitFor(() => {
			expect(gespeichert()).toEqual([{ ort: 'zuhause', varianten: ['fuenf-mwl', 'drei-isna'] }]);
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Seine Auswahl entfernen' }));
		await vi.waitFor(() => {
			expect(gespeichert()).toContainEqual({ ort: 'alt', varianten: [] });
		});
		expect(localStorage.length).toBe(0);
	});

	it('Gebetszeiten beim anderen Ort: die Auswahl geht nur in der Abfrage mit, nichts wird gespeichert', async () => {
		const f = server(TAG_GEBETE);
		render(Page);
		await screen.findByText('22:41');
		await fireEvent.input(screen.getByLabelText(/Breite/), { target: { value: '52.52' } });
		await fireEvent.input(screen.getByLabelText(/Länge/), { target: { value: '13.405' } });
		const form = screen.getByRole('group', { name: 'Anderer Ort' }).closest('form');
		if (form === null) throw new Error('Formular fehlt');
		await fireEvent.submit(form);
		await screen.findByText(/Gerechnet für Breite/);
		const schalter = screen.getByRole('switch', { name: 'Drei Zeiten, ISNA' });
		expect(schalter).toHaveAttribute('aria-checked', 'false');
		await fireEvent.click(schalter);
		await vi.waitFor(() => {
			const abfragen = f.mock.calls
				.filter((c) => String(c[0]) === '/api/kalender/abfrage')
				.map((c) => JSON.parse((c[1] as RequestInit).body as string) as Record<string, unknown>);
			expect(abfragen.at(-1)?.gebete).toEqual(['drei-isna']);
		});
		expect(
			f.mock.calls.some(
				(c) =>
					String(c[0]) === '/api/kalender/gebete' &&
					(c[1] as RequestInit | undefined)?.method === 'POST'
			)
		).toBe(false);
		expect(localStorage.length).toBe(0);
		expect(sessionStorage.length).toBe(0);
	});

	it('ohne Städteliste steht ein Satz da, die Namensnennung bleibt', async () => {
		const satz = 'Die Städteliste ist noch nicht eingerichtet; Breite und Länge gehen immer.';
		server(TAG, { treffer: [], quelle: STAEDTE.quelle, hinweis: satz });
		render(Page);
		await screen.findByText('04:43');
		await fireEvent.input(screen.getByLabelText('Stadt suchen'), {
			target: { value: 'Nebelfurt' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Suchen' }));
		expect(await screen.findByText(satz)).toBeInTheDocument();
		expect(screen.getAllByText(/Städteliste:/)).toHaveLength(1);
	});
});
