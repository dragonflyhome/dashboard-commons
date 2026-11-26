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
	KalenderOrt,
	KalenderOrte,
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
	hinweis: 'Die Zeiten gelten für die Mitte des Orts.'
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

/** Ein gefälschtes fetch, das je Pfad antwortet und jeden Aufruf mitschreibt. */
function server(tag: KalenderTag = TAG): ReturnType<typeof vi.fn> {
	let orte = ORTE;
	const f = vi.fn((pfad: string, init?: RequestInit) => {
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
		expect(await screen.findByText('Gemerkt als „Hafen“.')).toBeInTheDocument();

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
});
