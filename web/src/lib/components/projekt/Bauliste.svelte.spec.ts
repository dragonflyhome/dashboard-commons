import { fireEvent, render, screen } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import type * as Ladefehler from '#lib/ladefehler.ts';
import { rollenBeobachten } from './probe/rollen.ts';

// Schritt E2b: Die Bauliste kommt dynamisch; hier ein Platzhalter, der erst scheitert.
const { laden, teilFehlt } = vi.hoisted(() => ({ laden: vi.fn(), teilFehlt: vi.fn() }));
vi.mock('./bauliste-laden.ts', () => ({ bauliste: laden }));
vi.mock('#lib/ladefehler.ts', async (echt) => ({
	teilFehlt,
	fokusZeigen: (await echt<typeof Ladefehler>()).fokusZeigen
}));

const { default: Bauliste } = await import('./Bauliste.svelte');

const mit = createRawSnippet((liste: () => { projektSeiten: Record<string, unknown> }) => ({
	render: () => `<p>Liste mit ${String(Object.keys(liste().projektSeiten).length)} Seiten</p>`
}));

describe('Bauliste (Schritt E2b)', () => {
	it('zeigt das Fehlerbild, wenn sie sich nicht laden lässt und die Seite nicht selbst weitergeht; „neu versuchen“ lädt noch einmal', async () => {
		laden.mockReset();
		teilFehlt.mockReset();
		laden
			.mockRejectedValueOnce(new Error('kaputt'))
			.mockResolvedValueOnce({ projektSeiten: { a: vi.fn() }, projektUnterseiten: {} });
		teilFehlt.mockResolvedValue('fehler');
		render(Bauliste, { datei: 'projects/x/Seite.svelte', mit });
		expect(screen.getByText('Seite wird geladen …')).toBeInTheDocument();
		await fireEvent.click(await screen.findByRole('button', { name: 'neu versuchen' }));
		expect(await screen.findByText('Liste mit 1 Seiten')).toBeInTheDocument();
		expect(teilFehlt).toHaveBeenCalledTimes(1);
		expect(laden).toHaveBeenCalledTimes(2);
	});

	it('geht die Seite selbst weiter (Anmeldung, Neuladen), bleibt „Seite wird geladen …“ ohne Fehlerbild', async () => {
		laden.mockReset();
		teilFehlt.mockReset();
		laden.mockRejectedValue(new TypeError('Failed to fetch dynamically imported module: x'));
		teilFehlt.mockResolvedValue('weiter');
		render(Bauliste, { datei: 'projects/x/Seite.svelte', mit });
		await vi.waitFor(() => {
			expect(teilFehlt).toHaveBeenCalledTimes(1);
		});
		expect(screen.getByText('Seite wird geladen …')).toBeInTheDocument();
		expect(screen.queryByRole('alert')).toBeNull();
	});

	it('Anmeldung abgelaufen bei offener Eingabe: Hinweis mit Anmeldung in neuem Tab, „neu versuchen“ lädt noch einmal', async () => {
		laden.mockReset();
		teilFehlt.mockReset();
		laden
			.mockRejectedValueOnce(new TypeError('Failed to fetch dynamically imported module: x'))
			.mockResolvedValueOnce({ projektSeiten: {}, projektUnterseiten: {} });
		teilFehlt.mockResolvedValue('abgelaufen');
		const feld = document.body.appendChild(document.createElement('input'));
		feld.focus();
		const { rollen, hinweisDa, ende } = rollenBeobachten();
		render(Bauliste, { datei: 'projects/x/Seite.svelte', mit });
		const hinweis = await screen.findByRole('alert');
		// Nach dem Zeichnen des Hinweises das Feld mit dem Fokus ins Bild, der Fokus bleibt.
		await vi.waitFor(() => {
			expect(rollen).toHaveBeenCalledTimes(1);
		});
		ende();
		expect(rollen.mock.contexts[0]).toBe(feld);
		expect(rollen).toHaveBeenCalledWith({ block: 'nearest' });
		expect(hinweisDa).toEqual([true]);
		expect(document.activeElement).toBe(feld);
		feld.remove();
		expect(hinweis).toHaveTextContent('Anmeldung abgelaufen');
		expect(hinweis).not.toHaveTextContent('Diese Seite hat gerade einen Fehler');
		const link = screen.getByRole('link', { name: 'In neuem Tab anmelden' });
		expect(link).toHaveAttribute('href', '/anmelden');
		expect(link).toHaveAttribute('target', '_blank');
		expect(link).toHaveAttribute('rel', 'noopener');
		await fireEvent.click(screen.getByRole('button', { name: 'neu versuchen' }));
		expect(await screen.findByText('Liste mit 0 Seiten')).toBeInTheDocument();
		expect(laden).toHaveBeenCalledTimes(2);
	});

	it('fremdes Windows-Konto bei offener Eingabe: der Hinweis nennt das Konto', async () => {
		laden.mockReset();
		teilFehlt.mockReset();
		laden.mockRejectedValue(new TypeError('Failed to fetch dynamically imported module: x'));
		teilFehlt.mockResolvedValue('fremd');
		render(Bauliste, { datei: 'projects/x/Seite.svelte', mit });
		const hinweis = await screen.findByRole('alert');
		expect(hinweis).toHaveTextContent('Dieses Windows-Konto darf hier nicht.');
		expect(hinweis).not.toHaveTextContent('Anmeldung abgelaufen');
	});
});
