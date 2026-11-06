import { fireEvent, render, screen } from '@testing-library/svelte';
import * as svelteMount from 'svelte';
import { createRawSnippet } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import type { ProjektAntwort } from '$vertrag';

// Unter-Dashboard und Aufgaben laden selbst; hier zählt nur der Rahmen.
const { leer, ladenFuer } = vi.hoisted(() => ({
	leer: vi.fn(),
	ladenFuer: vi.fn(() => Promise.resolve())
}));
vi.mock('#lib/components/projekt/UnterDashboard.svelte', () => ({ default: leer }));
vi.mock('#lib/components/aufgaben/AufgabenKasten.svelte', () => ({ default: leer }));
vi.mock('#lib/aufgaben.svelte.ts', () => ({
	aufgaben: { benutzen: () => () => undefined, fuer: () => [], haken: vi.fn(), ladenFuer }
}));

const { default: NeueFormRahmen } = await import('./NeueFormRahmen.svelte');

function kopf(status = 'aktiv'): ProjektAntwort {
	return {
		slug: 'alpha',
		kopf: {
			title: 'Architektur',
			status,
			summary: '',
			chat: null,
			seiten: [{ name: 'zeitachse', title: 'Zeitachse', order: 1 }]
		},
		daten: []
	} as unknown as ProjektAntwort;
}

const inhalt = createRawSnippet(() => ({ render: () => '<p>Seite des Projekts</p>' }));

describe('Rahmen eines Projekts (Etappe 3b-2)', () => {
	it('kein Hinweis, kein „Zur Hauptseite“, Unterseiten direkt unter /projects/<slug>', () => {
		render(NeueFormRahmen, { kopf: kopf(), aktiv: null, children: inhalt });
		expect(screen.getByText('Seite des Projekts')).toBeInTheDocument();
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Architektur');
		expect(screen.queryByRole('link', { name: 'Zur Hauptseite' })).toBeNull();
		expect(screen.queryByText(/neue Ansicht/i)).toBeNull();
		// Den Weg tragen die Brotkrumen der Kopfleiste; keine eigene Zeile „Übersicht › …“ mehr.
		expect(document.querySelector('.crumbs')).toBeNull();
		expect(screen.queryByRole('link', { name: 'Übersicht' })?.closest('nav')).toHaveAccessibleName(
			'Seiten des Projekts'
		);
		const seiten = screen.getByRole('navigation', { name: 'Seiten des Projekts' });
		const links = [...seiten.querySelectorAll('a')].map((a) => a.getAttribute('href'));
		expect(links).toEqual(['/projects/alpha', '/projects/alpha/zeitachse']);
	});

	it('die gezeigte Unterseite ist markiert', () => {
		render(NeueFormRahmen, { kopf: kopf(), aktiv: 'zeitachse', children: inhalt });
		expect(screen.getByRole('link', { name: 'Zeitachse' })).toHaveAttribute('aria-current', 'page');
		expect(screen.getByRole('link', { name: 'Übersicht' })).not.toHaveAttribute('aria-current');
	});
});

describe('Rahmen fängt Fehler in Klick-Funktionen (Etappe 4b)', () => {
	it('ein Fehler beim Klicken zeigt das Fehlerbild; Kopf und Navigation bleiben; „neu versuchen“ zeichnet neu', async () => {
		const { default: KlickWirft } = await import('./probe/KlickWirft.svelte');
		const fehler = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		const seite = createRawSnippet(() => ({
			render: () => '<div></div>',
			setup: (ziel: Element) => {
				const { mount, unmount } = svelteMount;
				const k = mount(KlickWirft, { target: ziel });
				return () => {
					void unmount(k);
				};
			}
		}));
		render(NeueFormRahmen, { kopf: kopf(), aktiv: null, children: seite });
		const knopf = screen.getByRole('button', { name: 'Drück mich' });
		// Wie im Browser: Die Ausnahme der Klick-Funktion landet als `error` am Fenster.
		await fireEvent.click(knopf);
		const bild = await screen.findByRole('alert');
		expect(bild).toHaveTextContent('Diese Seite hat gerade einen Fehler');
		expect(bild).toHaveTextContent('Klick kaputt');
		expect(screen.queryByRole('button', { name: 'Drück mich' })).toBeNull();
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Architektur');
		expect(screen.getByRole('navigation', { name: 'Seiten des Projekts' })).toBeInTheDocument();
		await fireEvent.click(screen.getByRole('button', { name: 'neu versuchen' }));
		expect(await screen.findByRole('button', { name: 'Drück mich' })).toBeInTheDocument();
		expect(screen.queryByRole('alert')).toBeNull();
		fehler.mockRestore();
	});

	it('auch eine async-Funktion, die scheitert (unhandledrejection)', async () => {
		const fehler = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		render(NeueFormRahmen, { kopf: kopf(), aktiv: null, children: inhalt });
		const e = new Event('unhandledrejection', { cancelable: true }) as Event & { reason: unknown };
		e.reason = new Error('async kaputt');
		window.dispatchEvent(e);
		expect(await screen.findByRole('alert')).toHaveTextContent('async kaputt');
		expect(screen.queryByText('Seite des Projekts')).toBeNull();
		fehler.mockRestore();
	});
});

describe('Archiviertes Projekt (Entscheidung archiv-status)', () => {
	it('oben der ruhige Satz; die Aufgaben holt die Seite selbst', () => {
		ladenFuer.mockClear();
		render(NeueFormRahmen, {
			kopf: kopf('archiviert'),
			aktiv: null,
			children: inhalt
		});
		expect(screen.getByText('Dieses Projekt ist archiviert.')).toBeInTheDocument();
		expect(screen.getByText('Seite des Projekts')).toBeInTheDocument();
		expect(ladenFuer).toHaveBeenCalledWith('alpha');
	});

	it('aktiv: kein Satz, nichts eigens geholt', () => {
		ladenFuer.mockClear();
		render(NeueFormRahmen, { kopf: kopf(), aktiv: null, children: inhalt });
		expect(screen.queryByText('Dieses Projekt ist archiviert.')).toBeNull();
		expect(ladenFuer).not.toHaveBeenCalled();
	});
});
