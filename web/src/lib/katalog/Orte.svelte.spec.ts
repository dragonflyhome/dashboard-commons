/**
 * Orte mit dem Knopf „Öffnen“ (Entscheidung lokal-oeffnen): die
 * Oberfläche schickt nur Projekt und Kennung, nie einen Pfad, zeigt nie einen
 * Pfad und setzt keinen file:-Link; sie zeigt den Satz des Servers.
 */
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { Orte as OrteDaten } from '$vertrag';
import { Orte } from './index.ts';
import { fehlerSatz, oeffnen, SATZ_OHNE_ANMELDUNG } from '../oeffnen.ts';
import { ApiError } from '../api/client.ts';

const DATEN: OrteDaten = {
	ort: [
		{ kennung: 'bauplan', titel: 'Bauplan (Word)', hinweis: 'Fassung zum Ausdrucken' },
		{ kennung: 'fotos', titel: 'Ordner Fotos', hinweis: null }
	]
};

function antwort(status: number, koerper: unknown): Response {
	return new Response(JSON.stringify(koerper), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

describe('Orte', () => {
	it('je Ort Titel, Hinweis und ein Knopf „Öffnen“; kein Pfad, kein file:-Link, kein „Pfad kopieren“', () => {
		const { container } = render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn: vi.fn() });
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

	it('Klick: POST /api/oeffnen mit genau projekt und kennung, dann „Geöffnet.“', async () => {
		const fetchFn = vi.fn<typeof fetch>(() =>
			Promise.resolve(antwort(200, { projekt: 'beispiel', kennung: 'bauplan', satz: 'Geöffnet.' }))
		);
		render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await fireEvent.click(screen.getByRole('button', { name: 'Bauplan (Word) öffnen' }));
		expect(await screen.findByRole('status')).toHaveTextContent('Geöffnet.');
		expect(fetchFn).toHaveBeenCalledOnce();
		const [url, init] = fetchFn.mock.calls[0] ?? [];
		expect(url).toBe('/api/oeffnen');
		expect(init?.method).toBe('POST');
		expect(JSON.parse(init?.body as string)).toEqual({
			projekt: 'beispiel',
			kennung: 'bauplan'
		});
	});

	it('abgelehnt: der Satz des Servers als Hinweis, nur beim betroffenen Ort', async () => {
		const satz = 'Dateien mit der Endung .exe öffnet das Dashboard nicht.';
		const fetchFn = vi.fn<typeof fetch>(() =>
			Promise.resolve(antwort(422, { fehler: satz, art: 'ort-abgelehnt' }))
		);
		render(Orte, { daten: DATEN, projekt: 'beispiel', fetchFn });
		await fireEvent.click(screen.getByRole('button', { name: 'Ordner Fotos öffnen' }));
		expect(await screen.findByRole('alert')).toHaveTextContent(satz);
		expect(screen.getAllByRole('alert')).toHaveLength(1);
	});

	it('leere Datei: ein Satz statt einer Liste', () => {
		render(Orte, { daten: { ort: [] }, projekt: 'beispiel', fetchFn: vi.fn() });
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
});
