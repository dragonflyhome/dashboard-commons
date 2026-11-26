/**
 * Der Himmel oben rechts (Kalender, Stufe 6): Anzeige und Name, ausgeblendet
 * ohne Anmeldung und bei Fehler, Anmelden ohne Neuladen zeigt ihn, ein
 * Inhalts-Ereignis „kalender“ lädt neu, Abmelden leert sofort. Alle Werte erfunden.
 */
import { render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { KalenderKopf } from '$vertrag';
import type { InhaltAenderung } from '#lib/live.svelte.ts';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { ort } = vi.hoisted(() => ({ ort: { route: { id: '/' } } }));
vi.mock('$app/state', () => ({ page: ort }));
const { default: Himmel } = await import('./Himmel.svelte');
const { himmelSatz, mondPfad } = await import('#lib/himmel.svelte.ts');
const { live } = await import('#lib/live.svelte.ts');
const { freierKanal } = await import('#lib/freikanal.svelte.ts');

const KOPF: KalenderKopf = {
	ort: 'Mitte Deutschlands (Mühlhausen/Thüringen)',
	aufgang: '07:53',
	untergang: '16:18',
	text: null,
	phase_grad: 250,
	phase: 'abnehmend',
	sued: false,
	naechster_vollmond: '2026-12-24'
};
const SATZ =
	'Mond: abnehmend. Sonne heute 07:53 bis 16:18. Ort: Mitte Deutschlands (Mühlhausen/Thüringen). Zum Kalender';

function antwort(status: number, koerper: unknown): Response {
	return new Response(JSON.stringify(koerper), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}

/** Fängt die Hörer ab, die der Laden anmeldet. */
function hoerer() {
	const h = {
		inhalt: [] as ((a: InhaltAenderung) => void)[],
		bereit: [] as ((k: 'frei' | 'geschuetzt') => void)[],
		abgemeldet: [] as (() => void)[]
	};
	vi.spyOn(live, 'onInhalt').mockImplementation((l) => (h.inhalt.push(l), () => undefined));
	vi.spyOn(freierKanal, 'onBereit').mockImplementation((l) => (h.bereit.push(l), () => undefined));
	vi.spyOn(freierKanal, 'onAbgemeldet').mockImplementation(
		(l) => (h.abgemeldet.push(l), () => undefined)
	);
	return h;
}

afterEach(() => {
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
	ort.route.id = '/';
});

describe('Himmel in der Kopfleiste (Kalender, Stufe 6)', () => {
	it('zeigt Mond, Zeiten und Ort als Link zu /kalender mit dem vollen Satz', async () => {
		hoerer();
		const f = vi.fn(() => Promise.resolve(antwort(200, KOPF)));
		vi.stubGlobal('fetch', f);
		ort.route.id = '/kalender';
		render(Himmel);
		const link = await screen.findByRole('link', { name: SATZ });
		expect(link).toHaveAttribute('href', '/kalender');
		expect(link).toHaveAttribute('title', SATZ);
		expect(link).toHaveAttribute('aria-current', 'page');
		expect(link.textContent).toContain('↑ 07:53');
		expect(link.textContent).toContain('↓ 16:18');
		expect(link.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
		expect(f).toHaveBeenCalledTimes(1);
		expect(String(f.mock.calls[0]?.[0 as never])).toBe('/api/kalender/kopf');
		expect(localStorage.length + sessionStorage.length).toBe(0);
	});

	it('ohne Anmeldung (401) und bei Fehler oder ohne Netz zeichnet er nichts', async () => {
		hoerer();
		for (const f of [
			() => Promise.resolve(antwort(401, { fehler: 'x', art: 'anmeldung' })),
			() => Promise.resolve(antwort(500, { fehler: 'x' })),
			() => Promise.reject(new TypeError('kein Netz'))
		]) {
			const netz = vi.fn(f);
			vi.stubGlobal('fetch', netz);
			const { container, unmount } = render(Himmel);
			await waitFor(() => {
				expect(netz).toHaveBeenCalled();
			});
			await Promise.resolve();
			expect(container.querySelector('[data-himmel]')).toBeNull();
			expect(container.textContent.trim()).toBe('');
			unmount();
		}
	});

	it('Anmelden ohne Neuladen zeigt ihn; ein Ereignis „kalender“ lädt neu; Abmelden leert sofort', async () => {
		const h = hoerer();
		let angemeldet = false;
		let kopf = KOPF;
		const f = vi.fn(() =>
			Promise.resolve(angemeldet ? antwort(200, kopf) : antwort(401, { fehler: 'x' }))
		);
		vi.stubGlobal('fetch', f);
		const { container } = render(Himmel);
		await waitFor(() => {
			expect(f).toHaveBeenCalledTimes(1);
		});
		expect(container.querySelector('[data-himmel]')).toBeNull();
		angemeldet = true;
		for (const l of h.bereit) l('geschuetzt');
		expect(await screen.findByRole('link', { name: SATZ })).toBeInTheDocument();
		// Ein anderes Projekt lädt nicht neu, „kalender“ schon.
		kopf = {
			...KOPF,
			ort: 'Hütte',
			aufgang: null,
			untergang: null,
			text: 'Die Sonne geht heute nicht auf.'
		};
		for (const l of h.inhalt) l({ projekte: ['garten'], daten: [], version: 1, sammel: false });
		expect(f).toHaveBeenCalledTimes(2);
		for (const l of h.inhalt) l({ projekte: ['kalender'], daten: [], version: 2, sammel: false });
		const neu = await screen.findByRole('link', {
			name: 'Mond: abnehmend. Die Sonne geht heute nicht auf. Ort: Hütte. Zum Kalender'
		});
		expect(neu.textContent).not.toContain('↑');
		// Ohne Zeiten kein „·“ vor dem Ort.
		expect(neu.querySelector('.ort')?.textContent).toBe('Hütte');
		for (const l of h.abgemeldet) l();
		await waitFor(() => {
			expect(container.querySelector('[data-himmel]')).toBeNull();
		});
	});

	it('Mondpfad: Neumond ohne Licht, Vollmond ganz, abnehmend gespiegelt', () => {
		expect(mondPfad(0, 9).d).toContain('A 9.00 9 0 0 0 0 -9');
		expect(mondPfad(180, 9).d).toContain('A 9.00 9 0 0 1 0 -9');
		expect(mondPfad(90, 9).d).toContain('A 0.00 9');
		expect([mondPfad(45, 9).spiegeln, mondPfad(270, 9).spiegeln]).toEqual([false, true]);
		expect(
			himmelSatz({
				...KOPF,
				aufgang: null,
				untergang: null,
				text: 'Die Sonne geht heute nicht unter.'
			})
		).toBe(
			'Mond: abnehmend. Die Sonne geht heute nicht unter. Ort: Mitte Deutschlands (Mühlhausen/Thüringen). Zum Kalender'
		);
	});

	it('Übergangstag zu Polartag und -nacht: der Name nennt die eine Zeit und den Satz zur fehlenden Seite', () => {
		expect(
			himmelSatz({
				...KOPF,
				ort: 'Tromsø',
				untergang: null,
				text: 'Die Sonne geht heute nicht unter.'
			})
		).toBe(
			'Mond: abnehmend. Sonnenaufgang heute 07:53. Die Sonne geht heute nicht unter. Ort: Tromsø. Zum Kalender'
		);
		expect(
			himmelSatz({
				...KOPF,
				ort: 'Tromsø',
				aufgang: null,
				untergang: '00:17 (am 27.7.)',
				text: 'Für den Aufgang gibt es heute hier keine Zeit.'
			})
		).toBe(
			'Mond: abnehmend. Sonnenuntergang heute 00:17 (am 27.7.). Für den Aufgang gibt es heute hier keine Zeit. Ort: Tromsø. Zum Kalender'
		);
	});
});
