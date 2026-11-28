/**
 * Seite /heute (Kalender, Stufe 7b): Uhr, Sonne und Mond am Blickort, die Wahl
 * „Ich bin gerade in“, die Städtesuche (merken, dann wählen) und die Zustände
 * ohne Anmeldung, ohne Ort (404) und bei Fehler. Alle Werte erfunden.
 */
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { KalenderKopf, KalenderOrt, KalenderOrte, KalenderStaedte } from '$vertrag';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: Page } = await import('./+page.svelte');
const { SATZ_ANMELDUNG_NOETIG } = await import('#lib/anmeldung.ts');

const KOPF: KalenderKopf = {
	ort: 'Zuhause',
	kennung: 'zuhause',
	folgt_heimat: true,
	datum: '2026-11-27',
	zone: 'Europe/Berlin',
	aufgang: '07:53',
	untergang: '16:18',
	mittag: '12:05',
	aufgang_morgen: '07:54',
	hoehe: 12,
	hoehe_mittag: 18,
	naechster_tag: '2026-11-27T23:00:00Z',
	text: null,
	phase_grad: 250,
	phase: 'abnehmend',
	sued: false,
	naechster_vollmond: '2026-12-24'
};

const ORTE: KalenderOrte = {
	orte: [
		{ kennung: 'zuhause', titel: 'Zuhause', zeitart: 'Europe/Berlin', heimat: true },
		{ kennung: 'huette', titel: 'Hütte', zeitart: 'Europe/Oslo', heimat: false }
	]
};

const NEU: KalenderOrt = {
	kennung: 'ort-1',
	titel: 'Nebelfurt (XQ)',
	zeitart: 'America/Sao_Paulo',
	heimat: false
};

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

function json(status: number, body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

/** Ein gefälschtes fetch: Kopf mit `kopfStatus`, Wahl gibt den Kopf am gewählten Ort. */
function server(kopfStatus = 200, kopf: KalenderKopf = KOPF): ReturnType<typeof vi.fn> {
	let aktuell = kopf;
	const f = vi.fn((pfad: string, init?: RequestInit) => {
		if (pfad === '/api/kalender/kopf')
			return Promise.resolve(
				kopfStatus === 200
					? json(200, aktuell)
					: kopfStatus === 401
						? json(401, { fehler: 'Bitte anmelden.', art: 'anmeldung' })
						: kopfStatus === 404
							? json(404, { fehler: 'Es ist noch kein Ort eingetragen.' })
							: json(500, { fehler: 'Im Dashboard ist ein Fehler passiert.' })
			);
		if (pfad === '/api/kalender/blickort' && init?.method === 'POST') {
			const { ort } = JSON.parse(init.body as string) as { ort: string | null };
			aktuell =
				ort === null
					? kopf
					: {
							...kopf,
							kennung: ort,
							ort: ort === 'ort-1' ? NEU.titel : 'Hütte',
							folgt_heimat: false,
							zone: null
						};
			return Promise.resolve(json(200, aktuell));
		}
		if (pfad === '/api/kalender/standorte' && init?.method === 'POST')
			return Promise.resolve(json(200, NEU));
		if (pfad === '/api/kalender/standorte') return Promise.resolve(json(200, ORTE));
		if (pfad === '/api/kalender/staedte') return Promise.resolve(json(200, STAEDTE));
		return Promise.resolve(json(404, {}));
	});
	vi.stubGlobal('fetch', f);
	return f;
}

/** Wählt in der Auswahl „Ort“ den Eintrag `name`. */
async function ortWaehlen(name: string): Promise<void> {
	const wahl = screen.getByRole('combobox', { name: /^Ort / });
	wahl.focus();
	await fireEvent.keyDown(wahl, { key: 'ArrowDown' });
	const eintrag = await screen.findByRole('option', { name });
	await fireEvent.pointerDown(eintrag, { pointerType: 'mouse' });
	await fireEvent.pointerUp(eintrag, { pointerType: 'mouse' });
	await fireEvent.click(eintrag);
}

const posts = (f: ReturnType<typeof vi.fn>, pfad: string): unknown[] =>
	f.mock.calls
		.filter(([p, i]) => p === pfad && (i as RequestInit | undefined)?.method === 'POST')
		.map(([, i]) => JSON.parse((i as RequestInit).body as string) as unknown);

afterEach(() => {
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
});

describe('Seite /heute', () => {
	it('zeigt Uhr, Sonnenbogen und Mond am Blickort; die Auswahl beginnt mit „Zuhause (…)“', async () => {
		server();
		const { container } = render(Page);
		const bogen = await screen.findByRole('img', { name: /^Sonnenbogen: Aufgang 07:53/ });
		expect(bogen.getAttribute('aria-label')).toContain('Mittag 12:05 mit 18°');
		expect(screen.getByRole('img', { name: 'Mond: abnehmend' })).toBeInTheDocument();
		const uhr = container.querySelector('time');
		expect(uhr?.textContent).toMatch(/^\d{2}:\d{2}$/);
		expect(uhr).not.toHaveAttribute('aria-live');
		expect(screen.getByText(/Freitag, 27\. November 2026 · Zuhause/)).toBeInTheDocument();
		expect(screen.getByText('Nächster Vollmond: 24.12.2026', { exact: false })).toBeInTheDocument();
		expect(container.querySelector('[data-ohne-zone]')).toBeNull();
		const wahl = screen.getByRole('combobox', { name: /^Ort / });
		expect(wahl).toHaveTextContent('Zuhause (Zuhause)');
		expect(localStorage.length + sessionStorage.length).toBe(0);
	});

	it('eine Wahl wirkt sofort: Kennung im Rumpf, „Zuhause“ schickt null; ohne Zone steht der Satz', async () => {
		const f = server();
		const { container } = render(Page);
		await screen.findByRole('img', { name: /^Sonnenbogen/ });
		await ortWaehlen('Hütte');
		await waitFor(() => {
			expect(screen.getByText(/· Hütte$/)).toBeInTheDocument();
		});
		expect(container.querySelector('[data-ohne-zone]')?.textContent).toContain(
			'Für diesen Ort ist keine Zeitzone hinterlegt'
		);
		await ortWaehlen('Zuhause (Zuhause)');
		await waitFor(() => {
			expect(posts(f, '/api/kalender/blickort')).toEqual([{ ort: 'huette' }, { ort: null }]);
		});
		await waitFor(() => {
			expect(container.querySelector('[data-ohne-zone]')).toBeNull();
		});
	});

	it('Städtesuche: erst merken (Koordinaten nur im Rumpf), dann wählen; gesperrt, bis beides da ist', async () => {
		const f = server();
		render(Page);
		await screen.findByRole('img', { name: /^Sonnenbogen/ });
		expect(screen.getByText('Städteliste: GeoNames (geonames.org), CC BY 4.0')).toBeInTheDocument();
		await fireEvent.input(screen.getByLabelText('Stadt suchen'), { target: { value: 'Nebelf' } });
		await fireEvent.keyDown(screen.getByLabelText('Stadt suchen'), { key: 'Enter' });
		const knopf = await screen.findByRole('button', { name: 'Nebelfurt (XQ)' });
		await fireEvent.click(knopf);
		expect(knopf).toBeDisabled();
		await waitFor(() => {
			expect(posts(f, '/api/kalender/blickort')).toEqual([{ ort: 'ort-1' }]);
		});
		expect(posts(f, '/api/kalender/standorte')).toEqual([
			{ titel: 'Nebelfurt (XQ)', breite: 12.5, laenge: -33.75, zeitzone: 'America/Sao_Paulo' }
		]);
		expect(await screen.findByText(/· Nebelfurt \(XQ\)$/)).toBeInTheDocument();
		for (const [p] of f.mock.calls as [string][]) expect(p).not.toMatch(/12\.5|33\.75|Nebelf/);
		expect(localStorage.length + sessionStorage.length).toBe(0);
	});

	it('scheitert die Wahl nach dem Merken, steht die Meldung da', async () => {
		const echt = server();
		const f = vi.fn((p: string, i?: RequestInit) =>
			p === '/api/kalender/blickort'
				? Promise.resolve(json(422, { fehler: 'Diesen Ort gibt es nicht.' }))
				: (echt as unknown as typeof fetch)(p, i)
		);
		vi.stubGlobal('fetch', f);
		render(Page);
		await screen.findByRole('img', { name: /^Sonnenbogen/ });
		await fireEvent.input(screen.getByLabelText('Stadt suchen'), { target: { value: 'Nebelf' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Suchen' }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Nebelfurt (XQ)' }));
		expect(await screen.findByRole('alert')).toHaveTextContent('Diesen Ort gibt es nicht.');
		expect(posts(f, '/api/kalender/standorte')).toHaveLength(1);
	});

	it('ohne Anmeldung: der Satz zur Anmeldung, keine Uhr', async () => {
		server(401);
		const { container } = render(Page);
		expect(await screen.findByText(SATZ_ANMELDUNG_NOETIG)).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Zur Anmeldung' })).toHaveAttribute(
			'href',
			'/anmelden'
		);
		expect(container.querySelector('time')).toBeNull();
	});

	it('ohne Ort (404): der Satz des Servers mit der Städtesuche', async () => {
		server(404);
		const { container } = render(Page);
		expect(await screen.findByText('Es ist noch kein Ort eingetragen.')).toBeInTheDocument();
		expect(screen.getByLabelText('Stadt suchen')).toBeInTheDocument();
		expect(container.querySelector('time')).toBeNull();
		expect(screen.queryByRole('alert')).toBeNull();
	});

	it('bei einem Fehler: der Satz des Servers als Meldung', async () => {
		server(500);
		render(Page);
		const meldung = await screen.findByRole('alert');
		expect(meldung).toHaveTextContent('Im Dashboard ist ein Fehler passiert.');
		expect(within(meldung).queryByRole('img')).toBeNull();
	});
});
