import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import Beispiel from './Beispiel.svelte';

/**
 * jsdom misst nichts: Breite des Inhalts aus der Länge des Texts (10 px je
 * Zeichen der längsten Zeile), der Rahmen ist immer 300 px breit. Einen
 * ResizeObserver gibt es in jsdom nicht; gemessen wird also nur, weil sich
 * der Text ändert (wie nach einem Live-Neuladen mit gleich großem Rahmen).
 */
describe('Beispiel (System-Seiten)', () => {
	beforeEach(() => {
		vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get').mockImplementation(function (
			this: HTMLElement
		) {
			const zeilen = this.textContent.split('\n');
			return Math.max(...zeilen.map((z) => z.length)) * 10;
		});
		vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(300);
	});
	afterEach(() => {
		vi.restoreAllMocks();
	});

	it('kurz: kein Bereich, kein Tab-Stopp; reiner Text mit seinen Zeilen', () => {
		const { container } = render(Beispiel, { text: 'a\n  <b>b</b>', name: 'Aufruf' });
		const pre = container.querySelector('pre');
		expect(pre?.textContent).toBe('a\n  <b>b</b>');
		expect(container.querySelector('b')).toBeNull();
		expect(pre).not.toHaveAttribute('tabindex');
		expect(screen.queryByRole('region')).toBeNull();
	});

	it('neuer, breiterer Text bei gleichem Rahmen: neu gemessen, dann Bereich mit Name und Tab-Stopp', async () => {
		const { rerender } = render(Beispiel, { text: 'kurz', name: 'Aufruf' });
		expect(screen.queryByRole('region')).toBeNull();
		await rerender({ text: 'x'.repeat(80), name: 'Aufruf' });
		const r = screen.getByRole('region', { name: 'Aufruf' });
		expect(r.tagName).toBe('PRE');
		expect(r).toHaveAttribute('tabindex', '0');
		// Und zurück: wieder kurz, kein Bereich mehr.
		await rerender({ text: 'kurz', name: 'Aufruf' });
		expect(screen.queryByRole('region')).toBeNull();
	});
});
