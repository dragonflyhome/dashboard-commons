/**
 * Etappe S2: die Seite Einstellungen. Die Farbwahl steht hier als Karten
 * (Baustein `Wahl` in der Karten-Form) statt im Fuß jeder Seite; die Tests des
 * früheren Schalters sind hierher gewandert und prüfen dasselbe an der Seite.
 */
import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { THEMA_SCHLUESSEL, THEMEN } from '#lib/thema.ts';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: Page } = await import('./+page.svelte');

/** Eine gefälschte Abfrage des Systems (hell oder dunkel), mit Wechsel. */
function system(dunkel: boolean): { wechseln: (dunkel: boolean) => void } {
	const hoerer = new Set<(e: MediaQueryListEvent) => void>();
	const abfrage = {
		matches: dunkel,
		media: '(prefers-color-scheme: dark)',
		addEventListener: (_art: string, h: (e: MediaQueryListEvent) => void) => hoerer.add(h),
		removeEventListener: (_art: string, h: (e: MediaQueryListEvent) => void) => hoerer.delete(h)
	};
	vi.stubGlobal(
		'matchMedia',
		vi.fn(() => abfrage)
	);
	return {
		wechseln(neu) {
			abfrage.matches = neu;
			for (const h of hoerer) h({ matches: neu } as MediaQueryListEvent);
		}
	};
}

afterEach(() => {
	localStorage.clear();
	document.documentElement.removeAttribute('data-theme');
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
});

describe('Seite Einstellungen: Farben', () => {
	it('Gruppe „Farben“ mit den Optionsfeldern; die gemerkte Wahl ist gewählt', async () => {
		system(false);
		localStorage.setItem(THEMA_SCHLUESSEL, 'dunkel');
		render(Page);
		const gruppe = await screen.findByRole('radiogroup', { name: 'Farben' });
		expect(gruppe).toBeInTheDocument();
		const felder = screen.getAllByRole('radio');
		expect(felder.map((f) => f.getAttribute('aria-labelledby'))).toHaveLength(3);
		expect(felder.map((f) => screen.getByRole('radio', { name: namen(f) }))).toEqual(felder);
		expect(felder.map(namen)).toEqual(['Wie mein PC', 'Hell', 'Dunkel']);
		expect(await screen.findByRole('radio', { name: 'Dunkel' })).toHaveAttribute(
			'aria-checked',
			'true'
		);
	});

	it('Klick und Pfeiltaste wählen, merken und setzen data-theme; „Wie mein PC“ löscht die Wahl', async () => {
		system(false);
		render(Page);
		const hell = screen.getByRole('radio', { name: 'Hell' });
		await fireEvent.click(hell);
		expect(document.documentElement.getAttribute('data-theme')).toBe('hell');
		expect(localStorage.getItem(THEMA_SCHLUESSEL)).toBe('hell');
		hell.focus();
		await fireEvent.keyDown(hell, { key: 'ArrowRight' });
		expect(screen.getByRole('radio', { name: 'Dunkel' })).toHaveAttribute('aria-checked', 'true');
		expect(document.documentElement.getAttribute('data-theme')).toBe('dunkel');
		await fireEvent.click(screen.getByRole('radio', { name: 'Wie mein PC' }));
		expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
		expect(localStorage.getItem(THEMA_SCHLUESSEL)).toBeNull();
	});

	it('alle vier Pfeile wechseln und wenden sofort an', async () => {
		system(false);
		render(Page);
		const pc = screen.getByRole('radio', { name: 'Wie mein PC' });
		pc.focus();
		await fireEvent.keyDown(pc, { key: 'ArrowDown' });
		expect(document.documentElement.getAttribute('data-theme')).toBe('hell');
		const hell = screen.getByRole('radio', { name: 'Hell' });
		await fireEvent.keyDown(hell, { key: 'ArrowRight' });
		expect(document.documentElement.getAttribute('data-theme')).toBe('dunkel');
		const dunkel = screen.getByRole('radio', { name: 'Dunkel' });
		await fireEvent.keyDown(dunkel, { key: 'ArrowUp' });
		expect(document.documentElement.getAttribute('data-theme')).toBe('hell');
		await fireEvent.keyDown(screen.getByRole('radio', { name: 'Hell' }), { key: 'ArrowLeft' });
		expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
	});

	it('S1: die Seite zeigt genau eine Karte je Eintrag der Liste, in ihrer Reihenfolge', () => {
		system(false);
		render(Page);
		expect(screen.getAllByRole('radio').map(namen)).toEqual(THEMEN.map((t) => t.text));
	});

	it('der Name jedes Optionsfelds ist nur der Name der Karte; die Erklärung ist die Beschreibung', () => {
		system(false);
		render(Page);
		for (const t of THEMEN) {
			const feld = screen.getByRole('radio', { name: t.text });
			expect(feld).toHaveAccessibleName(t.text);
			// Im Knopf nur span-Elemente, nichts Bedienbares, keine Überschrift.
			expect(feld.querySelectorAll('*:not(span)')).toHaveLength(0);
			expect(feld.querySelectorAll('a, button, input, [tabindex]')).toHaveLength(0);
			expect(feld.querySelector('.v-muster')).toHaveAttribute('aria-hidden', 'true');
		}
		expect(screen.getByRole('radio', { name: 'Wie mein PC' })).toHaveAccessibleDescription(
			'Folgt deinem PC: hell oder dunkel, wie er gerade eingestellt ist. Gerade: Hell'
		);
		expect(screen.getByRole('radio', { name: 'Hell' })).toHaveAccessibleDescription('');
	});

	it('jede Karte trägt data-thema-vorschau mit ihrem Wert, „Wie mein PC“ den geltenden', async () => {
		const s = system(true);
		render(Page);
		const vorschau = (name: string): string | null =>
			screen
				.getByRole('radio', { name })
				.querySelector('[data-thema-vorschau]')
				?.getAttribute('data-thema-vorschau') ?? null;
		expect(vorschau('Hell')).toBe('hell');
		expect(vorschau('Dunkel')).toBe('dunkel');
		expect(await screen.findByText('Gerade: Dunkel')).toBeInTheDocument();
		expect(vorschau('Wie mein PC')).toBe('dunkel');
		// Der Rechner wechselt auf hell: Die Karte folgt.
		s.wechseln(false);
		expect(await screen.findByText('Gerade: Hell')).toBeInTheDocument();
		expect(vorschau('Wie mein PC')).toBe('hell');
		expect(vorschau('Dunkel')).toBe('dunkel');
	});

	it('nach der Wahl meldet die Zeile, was gilt; „Rückgängig“ stellt die vorige Wahl wieder her', async () => {
		system(false);
		localStorage.setItem(THEMA_SCHLUESSEL, 'hell');
		render(Page);
		const zeile = screen.getByRole('status');
		// Der Bereich steht von Anfang an leer da; den Knopf gibt es erst nach einer Wahl.
		expect(zeile.textContent).toBe('');
		expect(screen.queryByRole('button', { name: 'Rückgängig' })).toBeNull();
		await screen.findByRole('radio', { name: 'Hell', checked: true });

		await fireEvent.click(screen.getByRole('radio', { name: 'Dunkel' }));
		expect(zeile).toHaveTextContent('Farben: Dunkel.');
		const zurueck = screen.getByRole('button', { name: 'Rückgängig' });
		// Der Knopf steht neben dem Bereich, nicht darin.
		expect(zeile).not.toContainElement(zurueck);

		// Per Klick: zurück zu Hell.
		await fireEvent.click(zurueck);
		expect(document.documentElement.getAttribute('data-theme')).toBe('hell');
		expect(localStorage.getItem(THEMA_SCHLUESSEL)).toBe('hell');
		expect(zeile).toHaveTextContent('Farben: Hell.');
		expect(screen.getByRole('radio', { name: 'Hell' })).toHaveAttribute('aria-checked', 'true');
		// Der Knopf bleibt stehen und ist mit der Tastatur erreichbar: noch einmal, zurück zu Dunkel.
		const wieder = screen.getByRole('button', { name: 'Rückgängig' });
		expect(wieder.tabIndex).toBe(0);
		wieder.focus();
		expect(wieder).toHaveFocus();
		await fireEvent.keyDown(wieder, { key: 'Enter' });
		await fireEvent.click(wieder);
		expect(document.documentElement.getAttribute('data-theme')).toBe('dunkel');
		expect(zeile).toHaveTextContent('Farben: Dunkel.');
		expect(screen.getByRole('button', { name: 'Rückgängig' })).toHaveFocus();
	});

	it('„Rückgängig“ nach „Wie mein PC“ stellt die gemerkte Wahl wieder her', async () => {
		system(false);
		localStorage.setItem(THEMA_SCHLUESSEL, 'dunkel');
		render(Page);
		await screen.findByRole('radio', { name: 'Dunkel', checked: true });
		await fireEvent.click(screen.getByRole('radio', { name: 'Wie mein PC' }));
		expect(localStorage.getItem(THEMA_SCHLUESSEL)).toBeNull();
		expect(screen.getByRole('status')).toHaveTextContent('Farben: Wie mein PC.');
		await fireEvent.click(screen.getByRole('button', { name: 'Rückgängig' }));
		expect(localStorage.getItem(THEMA_SCHLUESSEL)).toBe('dunkel');
		expect(document.documentElement.getAttribute('data-theme')).toBe('dunkel');
	});
});

describe('Seite Einstellungen: Texte', () => {
	it('zwei Abschnitte mit den festen Sätzen', () => {
		system(false);
		render(Page);
		expect(screen.getByRole('heading', { level: 1, name: 'Einstellungen' })).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Aussehen' })).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Dieses Dashboard' })).toBeInTheDocument();
		for (const satz of [
			'Wähle, wie dein Dashboard aussieht. Die Wahl gilt sofort und nur in diesem Browser.',
			'Schriftgröße und weniger Bewegung kommen von deinem System.',
			'Dein Dashboard läuft nur hier auf diesem PC.',
			'Deine Wahl hier bleibt in diesem Browser und geht nirgendwohin.'
		]) {
			expect(screen.getByText(satz)).toBeInTheDocument();
		}
		// Kein Betriebssystem im Wortlaut.
		expect(document.body.textContent).not.toMatch(/Windows|macOS/);
	});
});

/** Der Name eines Optionsfelds, wie ihn `aria-labelledby` vorgibt. */
function namen(feld: HTMLElement): string {
	const id = feld.getAttribute('aria-labelledby') ?? '';
	return document.getElementById(id)?.textContent.trim() ?? '';
}
