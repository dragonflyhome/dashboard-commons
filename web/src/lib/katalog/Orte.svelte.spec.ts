/**
 * Orte mit dem Knopf „Öffnen“ (Entscheidung lokal-oeffnen): die
 * Oberfläche schickt nur Projekt und Kennung, nie einen Pfad, und zeigt den
 * Satz des Servers. Ohne Anmeldung kein Pfad und kein file:-Link; angemeldet
 * (Schritt E1b) der Pfad geprüfter Orte als Text und file:-Link, dessen Klick,
 * Mittelklick und Enter nur `POST /api/oeffnen` schicken. Alle Pfade erfunden.
 */
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Orte as OrteDaten, OrteVoll } from '$vertrag';
import { Orte } from './index.ts';
import { dateiAdresse, fehlerSatz, ladeOrte, oeffnen, SATZ_OHNE_ANMELDUNG } from '../oeffnen.ts';
import { ApiError } from '../api/client.ts';
import { freierKanal } from '../freikanal.svelte.ts';
import { zugang } from '../zugang.ts';

const DATEN: OrteDaten = {
	ort: [
		{ kennung: 'bauplan', titel: 'Bauplan (Word)', hinweis: 'Fassung zum Ausdrucken' },
		{ kennung: 'fotos', titel: 'Ordner Fotos', hinweis: null }
	]
};

const BAUPLAN = 'C:\\Users\\beispiel\\Dokumente\\Bauplan 1.docx';

/** Angemeldet: der Bauplan geprüft mit Pfad, die Fotos nicht. */
const VOLL: OrteVoll = {
	orte: [
		{ kennung: 'bauplan', titel: 'Bauplan (Word)', hinweis: null, geprueft: true, pfad: BAUPLAN },
		{ kennung: 'fotos', titel: 'Ordner Fotos', hinweis: null, geprueft: false, pfad: null }
	]
};

function antwort(status: number, koerper: unknown): Response {
	return new Response(JSON.stringify(koerper), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

/**
 * Ein Netz: `GET /api/projekte/beispiel/orte` gibt `orte` (oder 401, wenn
 * `null`), `POST /api/oeffnen` „Geöffnet.“.
 */
function netz(orte: () => OrteVoll | null) {
	return vi.fn<typeof fetch>((url, init) => {
		if (url === '/api/oeffnen' && init?.method === 'POST') {
			const k = JSON.parse(init.body as string) as { kennung: string };
			return Promise.resolve(
				antwort(200, { projekt: 'beispiel', kennung: k.kennung, satz: 'Geöffnet.' })
			);
		}
		if (url === '/api/projekte/beispiel/orte') {
			const o = orte();
			return Promise.resolve(
				o === null ? antwort(401, { fehler: 'x', art: 'anmeldung' }) : antwort(200, o)
			);
		}
		return Promise.resolve(antwort(404, { fehler: 'unbekannt' }));
	});
}

/** Die Körper aller POST an /api/oeffnen. */
function oeffnungen(f: ReturnType<typeof netz>): unknown[] {
	return f.mock.calls
		.filter(([url, init]) => url === '/api/oeffnen' && init?.method === 'POST')
		.map(([, init]) => JSON.parse(init?.body as string) as unknown);
}

/** Fängt den Hörer an `freierKanal.onAbgemeldet` ab; `ab` ist seine Abmeldung. */
function abmeldeHoerer(): { rufen: () => void; ab: ReturnType<typeof vi.fn> } {
	let hoerer: (() => void) | null = null;
	const ab = vi.fn();
	vi.spyOn(freierKanal, 'onAbgemeldet').mockImplementation((l) => {
		hoerer = l;
		return ab;
	});
	return {
		rufen: () => {
			expect(hoerer).not.toBeNull();
			hoerer?.();
		},
		ab
	};
}

/** Fängt den Hörer an `freierKanal.onBereit` ab; `ab` ist seine Abmeldung. */
function bereitHoerer(): {
	rufen: (k: 'frei' | 'geschuetzt') => void;
	ab: ReturnType<typeof vi.fn>;
} {
	let hoerer: ((k: 'frei' | 'geschuetzt') => void) | null = null;
	const ab = vi.fn();
	vi.spyOn(freierKanal, 'onBereit').mockImplementation((l) => {
		hoerer = l;
		return ab;
	});
	return {
		rufen: (k) => {
			expect(hoerer).not.toBeNull();
			hoerer?.(k);
		},
		ab
	};
}

/** Lässt angefangene Antworten (Körper lesen, Zustand setzen) zu Ende laufen. */
async function ruhe(): Promise<void> {
	await new Promise((r) => setTimeout(r, 20));
	await tick();
}

afterEach(() => {
	vi.restoreAllMocks();
});

describe('Orte', () => {
	it('ohne Anmeldung: Titel, Hinweis, Knopf „Öffnen“; kein Pfad, kein file:-Link, kein „Pfad kopieren“', async () => {
		const fetchFn = netz(() => null);
		const { container } = render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await waitFor(() => {
			expect(fetchFn).toHaveBeenCalledWith('/api/projekte/beispiel/orte', expect.anything());
		});
		expect(screen.getByText('Bauplan (Word)')).toBeInTheDocument();
		expect(screen.getByText('Fassung zum Ausdrucken')).toBeInTheDocument();
		expect(screen.getAllByRole('button', { name: /öffnen$/ })).toHaveLength(2);
		expect(screen.getByRole('button', { name: 'Ordner Fotos öffnen' })).toHaveTextContent('Öffnen');
		const html = container.innerHTML;
		expect(html).not.toMatch(/file:/i);
		expect(html).not.toMatch(/[A-Za-z]:\\/);
		expect(container.querySelectorAll('a')).toHaveLength(0);
		expect(screen.queryByText(/Pfad kopieren/)).toBeNull();
	});

	it('angemeldet: der Pfad als Text und als file:-Link, nur beim geprüften Ort', async () => {
		const { container } = render(Orte, {
			daten: DATEN,
			projekt: 'beispiel',
			fetchFn: netz(() => VOLL)
		});
		const link = await screen.findByRole('link', { name: BAUPLAN });
		expect(link).toHaveAttribute('href', 'file:///C:/Users/beispiel/Dokumente/Bauplan%201.docx');
		expect(container.innerHTML).toMatch(/file:/);
		expect(container.querySelectorAll('a')).toHaveLength(1);
	});

	it('href nur bei geprüft: ein Pfad ohne Prüfung und ein UNC-Pfad bleiben Text', async () => {
		const unc = '\\\\server\\freigabe\\Fotos';
		const fetchFn = netz(() => ({
			orte: [
				{ kennung: 'bauplan', titel: 'B', hinweis: null, geprueft: false, pfad: BAUPLAN },
				{ kennung: 'fotos', titel: 'F', hinweis: null, geprueft: true, pfad: unc }
			]
		}));
		const { container } = render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		expect(await screen.findByText(BAUPLAN)).toBeInTheDocument();
		expect(screen.getByText(unc)).toBeInTheDocument();
		expect(container.querySelectorAll('a')).toHaveLength(0);
		expect(container.innerHTML).not.toMatch(/file:/i);
	});

	it('Klick, Mittelklick und Enter auf den Link: nur POST /api/oeffnen, der Browser folgt dem href nicht', async () => {
		const fetchFn = netz(() => VOLL);
		render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		const link = await screen.findByRole('link', { name: BAUPLAN });
		// fireEvent gibt `false` zurück, wenn preventDefault gerufen wurde.
		expect(await fireEvent.click(link)).toBe(false);
		await waitFor(() => {
			expect(oeffnungen(fetchFn)).toHaveLength(1);
		});
		const mitte = new MouseEvent('auxclick', { button: 1, bubbles: true, cancelable: true });
		expect(await fireEvent(link, mitte)).toBe(false);
		await waitFor(() => {
			expect(oeffnungen(fetchFn)).toHaveLength(2);
		});
		expect(await fireEvent.keyDown(link, { key: 'Enter' })).toBe(false);
		await waitFor(() => {
			expect(oeffnungen(fetchFn)).toHaveLength(3);
		});
		for (const k of oeffnungen(fetchFn))
			expect(k).toEqual({ projekt: 'beispiel', kennung: 'bauplan' });
		// Die rechte Taste öffnet nichts (das Menü des Browsers bleibt).
		const rechts = new MouseEvent('auxclick', { button: 2, bubbles: true, cancelable: true });
		expect(await fireEvent(link, rechts)).toBe(true);
		expect(await screen.findByRole('status')).toHaveTextContent('Geöffnet.');
		expect(oeffnungen(fetchFn)).toHaveLength(3);
	});

	it('nach dem Abmelden (auch Widerruf, Ablauf): Pfade sofort weg, vor der neuen Antwort; nach dem Schließen kein Hörer', async () => {
		const hoerer = abmeldeHoerer();
		const offen: ((r: Response) => void)[] = [];
		let n = 0;
		// Die erste Anfrage antwortet mit Pfaden, jede weitere hängt, bis der Test sie löst.
		const fetchFn = vi.fn<typeof fetch>(() =>
			++n === 1 ? Promise.resolve(antwort(200, VOLL)) : new Promise<Response>((r) => offen.push(r))
		);
		const { container, unmount } = render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await screen.findByRole('link', { name: BAUPLAN });
		hoerer.rufen();
		// Die neue Anfrage hängt noch: der Pfad muss schon jetzt weg sein.
		await tick();
		expect(offen).toHaveLength(1);
		expect(container.querySelectorAll('a')).toHaveLength(0);
		expect(container.innerHTML).not.toMatch(/file:|[A-Za-z]:\\/i);
		expect(screen.getByText('Bauplan (Word)')).toBeInTheDocument();
		offen[0]?.(antwort(401, { fehler: 'x', art: 'anmeldung' }));
		await ruhe();
		expect(container.querySelectorAll('a')).toHaveLength(0);
		unmount();
		expect(hoerer.ab).toHaveBeenCalledOnce();
	});

	it('nach dem Abmelden bringt eine verspätete Antwort von vorher die Pfade nicht zurück', async () => {
		const hoerer = abmeldeHoerer();
		const offen: ((r: Response) => void)[] = [];
		const fetchFn = vi.fn<typeof fetch>(() => new Promise<Response>((r) => offen.push(r)));
		const { container } = render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await waitFor(() => {
			expect(offen).toHaveLength(1);
		});
		hoerer.rufen();
		expect(offen).toHaveLength(2);
		offen[1]?.(antwort(401, { fehler: 'x', art: 'anmeldung' }));
		await ruhe();
		// Die Antwort von vor dem Abmelden kommt erst jetzt, mit Pfaden.
		offen[0]?.(antwort(200, VOLL));
		await ruhe();
		expect(container.querySelectorAll('a')).toHaveLength(0);
		expect(container.innerHTML).not.toMatch(/file:|[A-Za-z]:\\/i);
	});

	it('angemeldet ohne Neuladen (Kanal wird geschützt): die Pfade kommen; nach dem Schließen beide Hörer ab', async () => {
		const abmelden = abmeldeHoerer();
		const bereit = bereitHoerer();
		let angemeldet = false;
		const fetchFn = netz(() => (angemeldet ? VOLL : null));
		const { container, unmount } = render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await waitFor(() => {
			expect(fetchFn).toHaveBeenCalledTimes(1);
		});
		angemeldet = true;
		bereit.rufen('frei');
		await ruhe();
		expect(fetchFn).toHaveBeenCalledTimes(1);
		expect(container.querySelectorAll('a')).toHaveLength(0);
		bereit.rufen('geschuetzt');
		expect(await screen.findByRole('link', { name: BAUPLAN })).toBeInTheDocument();
		unmount();
		expect(abmelden.ab).toHaveBeenCalledOnce();
		expect(bereit.ab).toHaveBeenCalledOnce();
	});

	it('Projektwechsel: die Pfade des alten Projekts sind sofort weg, vor der neuen Antwort', async () => {
		const offen: ((r: Response) => void)[] = [];
		const fetchFn = vi.fn<typeof fetch>((url) =>
			url === '/api/projekte/beispiel/orte'
				? Promise.resolve(antwort(200, VOLL))
				: new Promise<Response>((r) => offen.push(r))
		);
		const { container, rerender } = render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await screen.findByRole('link', { name: BAUPLAN });
		await rerender({ daten: DATEN, projekt: 'beispiel2', fetchFn });
		await tick();
		expect(fetchFn).toHaveBeenLastCalledWith('/api/projekte/beispiel2/orte', expect.anything());
		expect(offen).toHaveLength(1);
		expect(container.querySelectorAll('a')).toHaveLength(0);
		expect(container.innerHTML).not.toMatch(/file:|[A-Za-z]:\\/i);
	});

	it('Klick auf den Knopf: POST /api/oeffnen mit genau projekt und kennung, dann „Geöffnet.“', async () => {
		const fetchFn = netz(() => null);
		render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await fireEvent.click(screen.getByRole('button', { name: 'Bauplan (Word) öffnen' }));
		expect(await screen.findByRole('status')).toHaveTextContent('Geöffnet.');
		expect(oeffnungen(fetchFn)).toEqual([{ projekt: 'beispiel', kennung: 'bauplan' }]);
	});

	it('abgelehnt: der Satz des Servers als Hinweis, nur beim betroffenen Ort', async () => {
		const satz = 'Dateien mit der Endung .exe öffnet das Dashboard nicht.';
		const fetchFn = vi.fn<typeof fetch>((url) =>
			Promise.resolve(
				url === '/api/oeffnen'
					? antwort(422, { fehler: satz, art: 'ort-abgelehnt' })
					: antwort(401, { fehler: 'x', art: 'anmeldung' })
			)
		);
		render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await fireEvent.click(screen.getByRole('button', { name: 'Ordner Fotos öffnen' }));
		expect(await screen.findByRole('alert')).toHaveTextContent(satz);
		expect(screen.getAllByRole('alert')).toHaveLength(1);
	});

	it('leere Datei: ein Satz statt einer Liste', () => {
		render(Orte, { daten: { ort: [] }, projekt: 'beispiel', fetchFn: netz(() => null) });
		expect(screen.getByText(/Noch keine Orte/)).toBeInTheDocument();
	});

	it('ohne Projekt und außerhalb einer Projektseite: ein Fehler mit Satz', () => {
		expect(() => render(Orte, { daten: DATEN })).toThrow(/Orte geht nur in einer Projektseite/);
	});
});

describe('oeffnen.ts', () => {
	it('schickt nie einen Pfad, auch wenn die Kennung so aussieht (der Server sagt dann 422)', async () => {
		const fetchFn = vi.fn<typeof fetch>(() =>
			Promise.resolve(antwort(422, { fehler: 'Öffnen nimmt nur projekt und kennung.' }))
		);
		await expect(oeffnen('beispiel', 'x', fetchFn)).rejects.toBeInstanceOf(ApiError);
		const koerper = JSON.parse(fetchFn.mock.calls[0]?.[1]?.body as string) as Record<
			string,
			unknown
		>;
		expect(Object.keys(koerper).sort()).toEqual(['kennung', 'projekt']);
	});

	it('Sätze: 401 eigener Satz, 403/429/503 der Satz des Servers, ohne Netz ein eigener', () => {
		expect(fehlerSatz(new ApiError(401, 'x', 'anmeldung'))).toBe(SATZ_OHNE_ANMELDUNG);
		const konto = 'Öffnen geht nur in deinem eigenen Windows-Konto, dort erscheint das Fenster.';
		expect(fehlerSatz(new ApiError(403, konto, 'fremdes-konto'))).toBe(konto);
		expect(fehlerSatz(new ApiError(429, 'Höchstens 10 Öffnungen je Minute.', 'zu-viele'))).toBe(
			'Höchstens 10 Öffnungen je Minute.'
		);
		expect(fehlerSatz(new TypeError('netz'))).toMatch(/Keine Verbindung/);
	});

	it('dateiAdresse: nur geprüft und mit Laufwerk, nie UNC oder Gerätepfad; Teile kodiert', () => {
		const g = (pfad: string | null, geprueft = true) => dateiAdresse({ geprueft, pfad });
		expect(g('D:\\Ablage\\a#b?.pdf')).toBe('file:///D:/Ablage/a%23b%3F.pdf');
		expect(g('D:/Ablage/Ordner/')).toBe('file:///D:/Ablage/Ordner/');
		expect(g('D:\\Ablage\\a.pdf', false)).toBeNull();
		expect(g(null)).toBeNull();
		for (const p of [
			'\\\\server\\x\\a.pdf',
			'//server/x/a.pdf',
			'\\\\?\\D:\\a.pdf',
			'\\\\.\\D:\\a.pdf',
			'a.pdf'
		]) {
			expect(g(p), p).toBeNull();
		}
	});

	it('ladeOrte: null ohne Anmeldung, bei Fehlern und bei fremder Form', async () => {
		expect(
			await ladeOrte(
				'beispiel',
				netz(() => VOLL)
			)
		).toEqual(VOLL);
		expect(
			await ladeOrte(
				'beispiel',
				netz(() => null)
			)
		).toBeNull();
		expect(await ladeOrte('beispiel', () => Promise.reject(new TypeError('netz')))).toBeNull();
		expect(await ladeOrte('beispiel', () => Promise.resolve(antwort(200, { x: 1 })))).toBeNull();
	});

	it('ladeOrte: bei Zugang „fehlt“ geht keine Anfrage ins Netz', async () => {
		const fetchFn = netz(() => VOLL);
		zugang.setzen('fehlt');
		try {
			expect(await ladeOrte('beispiel', fetchFn)).toBeNull();
			expect(fetchFn).not.toHaveBeenCalled();
		} finally {
			zugang.vergessen();
		}
	});
});
