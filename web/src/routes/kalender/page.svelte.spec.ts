/**
 * Seite /kalender: Tageszeiten, Mond, Feste und „Anderer Ort“. Die
 * Koordinaten eines anderen Orts gehen nur im Rumpf eines POST, nie in die
 * Adresse.
 */
import { fireEvent, render, screen, within } from '@testing-library/svelte';
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
const { JAHR } = await import('#lib/textfassung.beispiele.ts');

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
	gebete: [],
	zaehlungen: []
};

// Erfundene Zeilen in der Form des Servers (Stufe 5).
const TAG_ZAEHLUNGEN: KalenderTag = {
	...TAG,
	zaehlungen: [
		{
			kennung: 'maya-584283',
			titel: 'Maya, Korrelation 584283',
			stellung: '13.0.14.2.8 · 9 Lamat · 1 Mak · Kalenderrunde 17048',
			teile: ['13.0.14.2.8', '9 Lamat', '1 Mak', 'Kalenderrunde 17048'],
			herkunft: 'Maya · Tzolkin · Korrelation 584283',
			stufe: 'belegt',
			hinweis: null
		},
		{
			kennung: 'dreamspell',
			titel: '13-Monde-Kalender (Dreamspell)',
			stellung: 'Mond 5 Tag 12 · Alpha · Kin 92 Yellow Magnetic Human',
			teile: ['Mond 5 Tag 12', 'Alpha', 'Kin 92 Yellow Magnetic Human'],
			herkunft: '13-Monde-Kalender (Dreamspell) · neuzeitlich',
			stufe: 'neuzeitlich',
			hinweis: 'Neuzeitliche Umdeutung.'
		}
	]
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
			// Wie vom Server: Die Herkunft beginnt mit dem Namen des Kalenders.
			herkunft: 'Jahreskreis · Sommersonnenwende',
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
	feste: FESTE,
	jahr: JAHR
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
			land: 'XQ',
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
	staedte: KalenderStaedte = STAEDTE,
	antwort: KalenderAbfrageAntwort = ANTWORT
): ReturnType<typeof vi.fn> {
	let orte = ORTE;
	const f = vi.fn((pfad: string, init?: RequestInit) => {
		if (pfad === '/api/kalender/heimat' && init?.method === 'POST') {
			const { ort } = JSON.parse(init.body as string) as { ort: string };
			orte = { orte: orte.orte.map((o) => ({ ...o, heimat: o.kennung === ort })) };
			return Promise.resolve(json(orte));
		}
		if (pfad.startsWith('/api/kalender/jahr')) return Promise.resolve(json(JAHR));
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
			return Promise.resolve(json(antwort));
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
		expect(screen.queryByText(/Jahreskreis · Jahreskreis/)).toBeNull();
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
		const stadt = await screen.findByRole('button', { name: 'Nebelfurt (XQ)' });
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

	// Gleichnamige Orte eines Lands (Koordinaten gerundet aus der Städteliste; Zwilling erfunden).
	const stadt = (name: string, land: string, breite: number, laenge: number) => ({
		name,
		land,
		region: '05',
		breite,
		laenge,
		zeitzone: 'Europe/Berlin'
	});
	const GLEICHNAMIG: KalenderStaedte = {
		treffer: [
			stadt('Frankfurt', 'DE', 50.11, 8.68),
			stadt('Frankfurt', 'DE', 52.34, 14.55),
			stadt('Frankfurt', 'US', 38.2, -84.87),
			stadt('Zwilling', 'DE', 48.13, 11.58),
			stadt('Zwilling', 'DE', 48.14, -9.9),
			// Gleiche angezeigte Breite trotz anderer Rundung (Süden und ,x5): dann auch die Länge.
			stadt('Kap', 'ZA', -34.25, 18.4),
			stadt('Kap', 'ZA', -34.31, 19.1),
			stadt('Rand', 'DE', 48.05, 7.1),
			stadt('Rand', 'DE', 48, 7.9),
			stadt('Nebelfurt', 'XQ', -12.5, 1),
			// Ohne Land: der Name allein, keine leeren Klammern.
			stadt('Leerland', '', 1, 1)
		],
		quelle: 'GeoNames (geonames.org), CC BY 4.0',
		hinweis: null
	};

	async function suchen(text: string): Promise<void> {
		await fireEvent.input(screen.getByLabelText('Stadt suchen'), { target: { value: text } });
		await fireEvent.keyDown(screen.getByLabelText('Stadt suchen'), { key: 'Enter' });
	}

	it('Städtesuche: Landname statt Code, gleichnamige Orte eines Lands mit Breite (und Länge)', async () => {
		server(TAG, GLEICHNAMIG);
		render(Page);
		await screen.findByText('04:43');
		await suchen('Frankfurt');
		await screen.findByRole('button', { name: /^Leerland/ });
		const namen = [...document.querySelectorAll('.treffer button')].map((b) =>
			b.textContent.trim()
		);
		expect(namen).toEqual([
			'Frankfurt (Deutschland, 50,1° N)',
			'Frankfurt (Deutschland, 52,3° N)',
			'Frankfurt (Vereinigte Staaten)',
			'Zwilling (Deutschland, 48,1° N, 11,6° O)',
			'Zwilling (Deutschland, 48,1° N, 9,9° W)',
			'Kap (Südafrika, 34,3° S, 18,4° O)',
			'Kap (Südafrika, 34,3° S, 19,1° O)',
			'Rand (Deutschland, 48,0° N, 7,1° O)',
			'Rand (Deutschland, 48,0° N, 7,9° O)',
			// Unbekannter Code: der Code selbst, nie leere Klammern.
			'Nebelfurt (XQ)',
			'Leerland'
		]);
		expect(new Set(namen).size).toBe(namen.length);
	});

	it('Städtesuche ohne Intl.DisplayNames (alte Engine): der Ländercode bleibt stehen', async () => {
		server(TAG, GLEICHNAMIG);
		vi.stubGlobal('Intl', { ...Intl, DisplayNames: undefined });
		render(Page);
		await screen.findByText('04:43');
		await suchen('Frankfurt');
		expect(
			await screen.findByRole('button', { name: 'Frankfurt (DE, 50,1° N)' })
		).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Frankfurt (US)' })).toBeInTheDocument();
	});

	it('Nach dem Rechnen steht der Name der gewählten Stadt statt „Abgefragter Ort“, nur in der Anzeige', async () => {
		// Wie der Server für einen abgefragten Ort: „ · Abgefragter Ort · <Zone>“ in jeder Herkunft
		// (src/api/kalender.rs), in Tageszeiten, Gebeten und Festen.
		const abgefragt = <T>(x: T): T =>
			JSON.parse(JSON.stringify(x).replaceAll('Ort Zuhause', 'Abgefragter Ort')) as T;
		const antwort: KalenderAbfrageAntwort = {
			...ANTWORT,
			tag: abgefragt({ ...TAG_GEBETE, ort: 'Abgefragter Ort', hinweis: null }),
			feste: {
				...FESTE,
				feste: FESTE.feste.map((x) => ({
					...x,
					herkunft: `${x.herkunft} · Abgefragter Ort · Europe/Berlin`
				}))
			}
		};
		const f = server(TAG, GLEICHNAMIG, antwort);
		render(Page);
		await screen.findByText('04:43');
		await suchen('Frankfurt');
		await fireEvent.click(
			await screen.findByRole('button', { name: 'Frankfurt (Deutschland, 52,3° N)' })
		);
		const form = screen.getByRole('group', { name: 'Anderer Ort' }).closest('form');
		if (form === null) throw new Error('Formular fehlt');
		await fireEvent.submit(form);
		expect(
			await screen.findByText(/^Frankfurt \(Deutschland, 52,3° N\) · Uhrzeiten in/)
		).toBeInTheDocument();
		expect(screen.queryByText(/Abgefragter Ort/)).toBeNull();
		// Derselbe Name in den Herkunftszeilen von Tageszeiten, Gebeten und Festen.
		const name = 'Frankfurt (Deutschland, 52,3° N)';
		expect(screen.getByText(`Refraktion 0,833° · ${name} · Europe/Berlin`)).toBeInTheDocument();
		expect(
			screen.getByText(`MWL 17° · Siebtel-Regel · ${name} · Europe/Berlin`)
		).toBeInTheDocument();
		expect(
			screen.getByText(`Jahreskreis · Sommersonnenwende · ${name} · Europe/Berlin`)
		).toBeInTheDocument();
		// Der Name geht nirgends an den Server: weder in eine Adresse noch in einen Rumpf.
		const abfragen = f.mock.calls.filter((c) => String(c[0]) === '/api/kalender/abfrage');
		expect(abfragen.length).toBeGreaterThan(0);
		for (const [p, init] of f.mock.calls as [string, RequestInit | undefined][]) {
			expect(p).not.toMatch(/Deutschland/);
			const rumpf = typeof init?.body === 'string' ? init.body : '';
			if (p !== '/api/kalender/staedte') expect(rumpf).not.toMatch(/Frankfurt|Deutschland/);
		}
		expect(localStorage.length).toBe(0);
		expect(sessionStorage.length).toBe(0);

		// Von Hand geänderte Breite: das ist nicht mehr die Stadt, also wieder „Abgefragter Ort“.
		await fireEvent.input(screen.getByLabelText(/Breite/), { target: { value: '52.4' } });
		await fireEvent.submit(form);
		expect(await screen.findByText(/^Abgefragter Ort · Uhrzeiten in/)).toBeInTheDocument();
		expect(screen.queryByText(/Frankfurt \(Deutschland/)).toBeNull();

		// „Zurück“ vergisst die Stadt: dieselben Werte von Hand ergeben „Abgefragter Ort“.
		await fireEvent.click(screen.getByRole('button', { name: /^Zurück zu/ }));
		await fireEvent.input(screen.getByLabelText(/Breite/), { target: { value: '52.34' } });
		await fireEvent.input(screen.getByLabelText(/Länge/), { target: { value: '14.55' } });
		await fireEvent.input(screen.getByLabelText(/Zeitzone/), {
			target: { value: 'Europe/Berlin' }
		});
		await fireEvent.submit(form);
		expect(await screen.findByText(/^Abgefragter Ort · Uhrzeiten in/)).toBeInTheDocument();
		expect(screen.queryByText(/Frankfurt \(Deutschland/)).toBeNull();
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

	it('Gebetszeiten: zwei Klicks vor der ersten Antwort gehen beide in die Auswahl', async () => {
		const offen: (() => void)[] = [];
		const sonst = server(TAG_GEBETE) as unknown as (
			p: string,
			i?: RequestInit
		) => Promise<Response>;
		// Wie server(), nur hält es jede Antwort auf das Speichern zurück, bis der Test sie freigibt.
		const f = vi.fn((pfad: string, init?: RequestInit): Promise<Response> => {
			if (pfad === '/api/kalender/gebete' && init?.method === 'POST') {
				const k = JSON.parse(init.body as string) as { ort: string; varianten: string[] };
				const antwort = { ...GEBETE, auswahl: [{ ...k, fehlt: false }] };
				return new Promise<Response>((r) => {
					offen.push(() => {
						r(json(antwort));
					});
				});
			}
			return sonst(pfad, init);
		});
		vi.stubGlobal('fetch', f);
		const gespeichert = () =>
			f.mock.calls
				.filter(([p, init]) => p === '/api/kalender/gebete' && init?.method === 'POST')
				.map(([, init]) => JSON.parse(init?.body as string) as unknown);
		render(Page);
		await screen.findByText('22:41');
		const drei = screen.getByRole('switch', { name: 'Drei Zeiten, ISNA' });
		const fuenf = screen.getByRole('switch', { name: 'Fünf Zeiten, MWL' });
		await fireEvent.click(drei);
		await fireEvent.click(fuenf);
		// Der zweite Klick wartet auf die erste Antwort und baut auf dem ersten auf.
		expect(gespeichert()).toEqual([{ ort: 'zuhause', varianten: ['fuenf-mwl', 'drei-isna'] }]);
		offen.shift()?.();
		await vi.waitFor(() => {
			expect(gespeichert()).toHaveLength(2);
		});
		expect(gespeichert()[1]).toEqual({ ort: 'zuhause', varianten: ['drei-isna'] });
		offen.shift()?.();
		await vi.waitFor(() => {
			expect(fuenf).toHaveAttribute('aria-checked', 'false');
			expect(drei).toHaveAttribute('aria-checked', 'true');
		});
		expect(gespeichert()).toHaveLength(2);
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

	it('Zählungen: ein Abschnitt in der Reihenfolge des Servers mit Stellung, Stufe, Hinweis und Herkunft', async () => {
		server(TAG_ZAEHLUNGEN);
		const { container } = render(Page);
		expect(await screen.findByRole('heading', { name: 'Zählungen' })).toBeInTheDocument();
		const zeilen = [...container.querySelectorAll('[data-zaehlung]')].map((z) =>
			z.getAttribute('data-zaehlung')
		);
		expect(zeilen).toEqual(['maya-584283', 'dreamspell']);
		expect(screen.getByText('Maya, Korrelation 584283')).toBeInTheDocument();
		expect(screen.getByText(/13\.0\.14\.2\.8 · 9 Lamat · 1 Mak/)).toBeInTheDocument();
		expect(screen.getByText('(neuzeitlich)')).toBeInTheDocument();
		expect(screen.queryByText('(belegt)')).toBeNull();
		expect(screen.getByText('Neuzeitliche Umdeutung.')).toBeInTheDocument();
		expect(screen.getByText('Maya · Tzolkin · Korrelation 584283')).toBeInTheDocument();
	});

	it('Zählungen: ohne aktive Zählung kein Abschnitt', async () => {
		server();
		render(Page);
		await screen.findByText('04:43');
		expect(screen.queryByRole('heading', { name: 'Zählungen' })).toBeNull();
	});

	it('Gebetszeiten: Wünsche je Ort; ein Fehlschlag zeigt den gespeicherten Stand, der Fokus bleibt', async () => {
		const offen: ((ok: boolean) => void)[] = [];
		const sonst = server(TAG_GEBETE) as unknown as (
			p: string,
			i?: RequestInit
		) => Promise<Response>;
		// Hält jede Antwort auf das Speichern zurück; der Test gibt sie als Erfolg oder Fehlschlag frei.
		const f = vi.fn((pfad: string, init?: RequestInit): Promise<Response> => {
			if (pfad === '/api/kalender/gebete' && init?.method === 'POST') {
				const k = JSON.parse(init.body as string) as { ort: string; varianten: string[] };
				return new Promise<Response>((r) => {
					offen.push((ok) => {
						r(
							ok
								? json({ ...GEBETE, auswahl: [{ ...k, fehlt: false }] })
								: new Response(JSON.stringify({ fehler: 'Nicht gespeichert.' }), { status: 409 })
						);
					});
				});
			}
			return sonst(pfad, init);
		});
		vi.stubGlobal('fetch', f);
		const gespeichert = () =>
			f.mock.calls
				.filter(([p, init]) => p === '/api/kalender/gebete' && init?.method === 'POST')
				.map(([, init]) => JSON.parse(init?.body as string) as unknown);
		render(Page);
		await screen.findByText('22:41');
		// Klick am Heimatort, Ortswechsel und Klick an der Hütte, bevor die erste Antwort kommt.
		await fireEvent.click(screen.getByRole('switch', { name: 'Drei Zeiten, ISNA' }));
		const ortWahl = screen.getByRole('combobox', { name: /^Ort / });
		ortWahl.focus();
		await fireEvent.keyDown(ortWahl, { key: 'ArrowDown' });
		const huette = await screen.findByRole('option', { name: 'Hütte' });
		await fireEvent.pointerDown(huette, { pointerType: 'mouse' });
		await fireEvent.pointerUp(huette, { pointerType: 'mouse' });
		await fireEvent.click(huette);
		await vi.waitFor(() => {
			expect(f.mock.calls.some(([p]) => p.includes('ort=h%C3%BCtte'))).toBe(true);
		});
		await fireEvent.click(screen.getByRole('switch', { name: 'Fünf Zeiten, MWL' }));
		offen.shift()?.(true);
		await vi.waitFor(() => {
			expect(gespeichert()).toHaveLength(2);
		});
		expect(gespeichert()).toEqual([
			{ ort: 'zuhause', varianten: ['fuenf-mwl', 'drei-isna'] },
			{ ort: 'hütte', varianten: ['fuenf-mwl'] }
		]);
		// Der zweite schlägt fehl: Der Schalter zeigt wieder den gespeicherten Stand und behält den Fokus.
		const fuenf = screen.getByRole('switch', { name: 'Fünf Zeiten, MWL' });
		fuenf.focus();
		offen.shift()?.(false);
		await vi.waitFor(() => {
			expect(screen.getByRole('alert')).toBeInTheDocument();
			expect(fuenf).toHaveAttribute('aria-checked', 'false');
		});
		expect(document.activeElement).toBe(fuenf);
		expect(fuenf.isConnected).toBe(true);
	});

	it('Stufe 6: „Als Heimatort“ nur am Ort, der es nicht ist; danach steht „(Heimat)“ dort und der Knopf ist weg', async () => {
		const f = server();
		render(Page);
		await screen.findByText('04:43');
		expect(screen.queryByRole('button', { name: 'Als Heimatort' })).toBeNull();
		const ortWahl = screen.getByRole('combobox', { name: 'Ort Zuhause (Heimat)' });
		ortWahl.focus();
		await fireEvent.keyDown(ortWahl, { key: 'ArrowDown' });
		const huette = await screen.findByRole('option', { name: 'Hütte' });
		await fireEvent.pointerDown(huette, { pointerType: 'mouse' });
		await fireEvent.pointerUp(huette, { pointerType: 'mouse' });
		await fireEvent.click(huette);
		await fireEvent.click(await screen.findByRole('button', { name: 'Als Heimatort' }));
		// Die Hütte bleibt gewählt und heißt jetzt „(Heimat)“; der Knopf ist weg.
		expect(await screen.findByRole('combobox', { name: 'Ort Hütte (Heimat)' })).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'Als Heimatort' })).toBeNull();
		const post = f.mock.calls.find((c) => String(c[0]) === '/api/kalender/heimat');
		expect(JSON.parse((post?.[1] as RequestInit).body as string)).toEqual({ ort: 'hütte' });
		// Der Fokus fällt nicht auf body: Er steht wie bei „Ort merken“ auf der Rückmeldung.
		const meldung = await screen.findByText('Heimatort ist jetzt „Hütte“.');
		expect(meldung).toHaveAttribute('role', 'status');
		expect(meldung).toHaveAttribute('tabindex', '-1');
		expect(document.activeElement).toBe(meldung);
	});

	it('Stufe 6: ein geleertes Datumsfeld lässt den Jahreskreis beim geladenen Tag (kein Zeiger mit NaN)', async () => {
		server();
		const { container } = render(Page);
		const abschnitt = (await screen.findByRole('heading', { name: 'Das Jahr' })).closest('section');
		if (abschnitt === null) throw new Error('Abschnitt fehlt');
		const feld = container.querySelector<HTMLInputElement>('input[type="date"]');
		if (feld === null) throw new Error('Datumsfeld fehlt');
		await fireEvent.input(feld, { target: { value: '' } });
		expect(feld.value).toBe('');
		const bild = within(abschnitt).getByRole('img');
		const zeiger = bild.querySelector('path.zeiger')?.getAttribute('d') ?? '';
		expect(zeiger).toMatch(/^M [\d.-]+ [\d.-]+ L [\d.-]+ [\d.-]+$/);
		expect(zeiger).not.toContain('NaN');
		// Der Name nennt den geladenen Tag (21.6.2026), nicht „am “.
		expect(bild.getAttribute('aria-label')).toMatch(/, am .*2026$/);
	});

	it('Stufe 6: „Das Jahr“ mit Kreis (role=img, Name), Liste der Monate, Sonnentage, nächste Vollmonde und Textfassung', async () => {
		const f = server();
		const { container } = render(Page);
		const abschnitt = (await screen.findByRole('heading', { name: 'Das Jahr' })).closest('section');
		if (abschnitt === null) throw new Error('Abschnitt fehlt');
		const bild = within(abschnitt).getByRole('img');
		expect(bild.getAttribute('aria-label')).toMatch(/^Jahreskreis 2026\/27, 12 Mondmonate, am /);
		expect(bild.getAttribute('viewBox')).toBe('0 0 400 400');
		expect(bild.querySelectorAll('path.monat')).toHaveLength(12);
		expect(bild.querySelectorAll('circle.voll')).toHaveLength(12);
		expect(bild.querySelectorAll('path.marke')).toHaveLength(4);
		expect(bild.querySelectorAll('path.zeit')).toHaveLength(4);
		expect(bild.querySelectorAll('path.zeiger')).toHaveLength(1);
		const listen = [...abschnitt.querySelectorAll('ol')];
		expect(listen.map((l) => l.children.length).slice(0, 2)).toEqual([12, 4]);
		const [monate, sonne] = listen;
		if (monate === undefined || sonne === undefined) throw new Error('Listen fehlen');
		expect(within(monate).getByText(/2\. Wintervollmond · Yule-Mond/)).toBeInTheDocument();
		expect(within(sonne).getByText('Wintersonnenwende')).toBeInTheDocument();
		const text = container.querySelector('[data-textfassung] table');
		expect(text?.querySelector('caption')?.textContent).toBe('Jahreskreis 2026/27 als Text');
		expect(text?.querySelectorAll('tbody tr').length).toBeGreaterThanOrEqual(16);
		// Der Ort geht nur als Kennung in die Adresse, wie bei Tag und Festen.
		const pfad = f.mock.calls
			.map((c) => String(c[0]))
			.find((p) => p.startsWith('/api/kalender/jahr'));
		expect(pfad).toMatch(/^\/api\/kalender\/jahr\?datum=\d{4}-\d{2}-\d{2}&ort=zuhause$/);
	});

	it('Stufe 6: beim anderen Ort kommt das Jahr aus derselben Abfrage (jahr = true im Rumpf)', async () => {
		const f = server();
		render(Page);
		await screen.findByText('04:43');
		await fireEvent.input(screen.getByLabelText(/Breite/), { target: { value: '52.52' } });
		await fireEvent.input(screen.getByLabelText(/Länge/), { target: { value: '13.405' } });
		const form = screen.getByRole('group', { name: 'Anderer Ort' }).closest('form');
		if (form === null) throw new Error('Formular fehlt');
		await fireEvent.submit(form);
		await screen.findByText(/Gerechnet für Breite/);
		const post = f.mock.calls.find((c) => String(c[0]) === '/api/kalender/abfrage');
		expect(JSON.parse((post?.[1] as RequestInit).body as string)).toMatchObject({ jahr: true });
		expect(screen.getByRole('heading', { name: 'Das Jahr' })).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'Als Heimatort' })).toBeNull();
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
