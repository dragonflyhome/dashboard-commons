import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { FassungsEintrag, NeuIn as NeuInAntwort } from '$vertrag';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: NeuIn } = await import('./NeuIn.svelte');

const EINTRAG: FassungsEintrag = {
	version: '0.3.0',
	tag: 'v0.3.0',
	datum: '2026-11-20',
	hinzugefuegt: ['Seite /fassungen.'],
	geaendert: ['Bruch: Alte Form weg.'],
	entfernt: [],
	behoben: [],
	brueche: ['Bruch: Alte Form weg.']
};

describe('Neu in …', () => {
	it('zeigt die neue Fassung einmal und merkt sie als gezeigt', async () => {
		const main = document.createElement('main');
		main.id = 'inhalt';
		main.tabIndex = -1;
		document.body.append(main);
		const merken = vi.fn(() => Promise.resolve({ neu_in: null }));
		render(NeuIn, {
			laden: () => Promise.resolve({ neu_in: EINTRAG } satisfies NeuInAntwort),
			merken
		});
		expect(await screen.findByRole('region', { name: 'Neu in 0.3.0' })).toBeInTheDocument();
		// Keine Überschrift: Der Kasten steht vor der h1 der Seite.
		expect(screen.queryByRole('heading')).toBeNull();
		expect(screen.getByText('Seite /fassungen.')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Alle Fassungen' })).toHaveAttribute(
			'href',
			'/fassungen'
		);
		await waitFor(() => {
			expect(merken).toHaveBeenCalledTimes(1);
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Schließen' }));
		expect(screen.queryByRole('region', { name: 'Neu in 0.3.0' })).toBeNull();
		expect(document.activeElement).toBe(main);
		main.remove();
	});

	it('nichts Neues, kein Zugang oder Fehler: kein Kasten, nichts gemerkt', async () => {
		for (const laden of [
			() => Promise.resolve({ neu_in: null }),
			() => Promise.reject(new Error('401'))
		]) {
			const merken = vi.fn(() => Promise.resolve({}));
			const { unmount } = render(NeuIn, { laden, merken });
			await new Promise((r) => setTimeout(r, 0));
			expect(screen.queryByRole('region')).toBeNull();
			expect(merken).not.toHaveBeenCalled();
			unmount();
		}
	});
});
