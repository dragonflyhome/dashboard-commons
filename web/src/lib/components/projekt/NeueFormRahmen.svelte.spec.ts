import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import * as svelteMount from 'svelte';
import { createRawSnippet } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import type { ProjektAntwort } from '$vertrag';

// Die vier Teile laden selbst (eigene Tests); hier zählt nur der Rahmen.
const { teile, ladenFuer } = vi.hoisted(() => ({
	teile: vi.fn(),
	ladenFuer: vi.fn(() => Promise.resolve())
}));
vi.mock('#lib/components/projekt/ProjektTeile.svelte', () => ({ default: teile }));
// Der Bau von „alpha“ ist rot (BauKopf liest den gemeinsamen Stand).
vi.mock('#lib/bau.svelte.ts', () => ({
	bauLage: {
		kopf: () => ({ zustand: 'rot', bedeutung: 'problem', zeile: 'Bau rot', satz: null }),
		schritt: null,
		standMelden: () => undefined
	}
}));
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
		teile.mockClear();
		render(NeueFormRahmen, { kopf: kopf(), aktiv: null, children: inhalt });
		expect(screen.getByText('Seite des Projekts')).toBeInTheDocument();
		// Etappe B8: das Status-Wort einmal, in der Überschrift; keine zweite Zeile „Status: …“.
		expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(/^Architektur\s*aktiv$/);
		expect(screen.queryByText(/^Status:/)).toBeNull();
		// Die vier Teile stehen auf der Übersicht.
		expect(teile).toHaveBeenCalledTimes(1);
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

	it('die gezeigte Unterseite ist markiert; dort stehen die vier Teile nicht', () => {
		teile.mockClear();
		render(NeueFormRahmen, { kopf: kopf(), aktiv: 'zeitachse', children: inhalt });
		expect(screen.getByRole('link', { name: 'Zeitachse' })).toHaveAttribute('aria-current', 'page');
		expect(screen.getByRole('link', { name: 'Übersicht' })).not.toHaveAttribute('aria-current');
		expect(teile).not.toHaveBeenCalled();
	});

	it('wirft ein Teil beim Zeichnen, bleibt der Inhalt der Seite sichtbar', () => {
		const fehler = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		teile.mockImplementationOnce(() => {
			throw new TypeError("Cannot read properties of undefined (reading 'length')");
		});
		render(NeueFormRahmen, { kopf: kopf(), aktiv: null, children: inhalt });
		expect(screen.getByText('Seite des Projekts')).toBeInTheDocument();
		expect(screen.getByRole('status')).toHaveTextContent(
			'Stand, Aufgabenliste, Plan und Zeitleiste lassen sich mit dem laufenden Dienst gerade nicht zeigen.'
		);
		expect(fehler).toHaveBeenCalled();
		fehler.mockRestore();
		// Gegenprobe: ohne Fehler kein Satz.
		cleanup();
		render(NeueFormRahmen, { kopf: kopf(), aktiv: null, children: inhalt });
		expect(screen.queryByRole('status')).toBeNull();
	});

	it('der Fehler der Teile gilt nicht für das nächste Projekt, „neu versuchen“ zeichnet neu', async () => {
		const fehler = vi.spyOn(console, 'error').mockImplementation(() => undefined);
		const werfen = (): never => {
			throw new TypeError("Cannot read properties of undefined (reading 'length')");
		};
		teile.mockReset();
		teile.mockImplementationOnce(werfen);
		const { rerender } = render(NeueFormRahmen, { kopf: kopf(), aktiv: null, children: inhalt });
		expect(screen.getByRole('status')).toBeInTheDocument();
		// Ein anderes Projekt auf derselben Route: die Teile zeichnen neu, kein Satz.
		await rerender({ kopf: { ...kopf(), slug: 'beta' }, aktiv: null, children: inhalt });
		expect(screen.queryByRole('status')).toBeNull();
		expect(teile).toHaveBeenCalledTimes(2);
		// Wieder ein Fehler, dann „neu versuchen“: die Teile zeichnen noch einmal.
		teile.mockImplementationOnce(werfen);
		await rerender({ kopf: kopf(), aktiv: null, children: inhalt });
		expect(screen.getByRole('status')).toBeInTheDocument();
		await fireEvent.click(screen.getByRole('button', { name: 'neu versuchen' }));
		expect(screen.queryByRole('status')).toBeNull();
		expect(teile).toHaveBeenCalledTimes(4);
		fehler.mockRestore();
	});

	it('rot auf einer Unterseite: der Bau-Hinweis bleibt, ohne die vier Teile', () => {
		render(NeueFormRahmen, { kopf: kopf(), aktiv: 'zeitachse', children: inhalt });
		expect(document.querySelector('[data-bau="rot"]')).toHaveTextContent('Bau rot');
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
