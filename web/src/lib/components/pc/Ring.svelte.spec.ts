import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { Belegung } from '$vertrag';
import Ring from './Ring.svelte';

describe('Ring', () => {
	it.each([0, 50, 100])('%i %%: in der Mitte nur die Zahl, der Wert steht unter dem Ring', (p) => {
		const { container } = render(Ring, {
			percent: p,
			belegung: 'normal',
			label: 'Arbeitsspeicher',
			value: '12,3 GB / 31,7 GB'
		});
		const texte = [...container.querySelectorAll('svg text')];
		expect(texte).toHaveLength(1);
		expect(texte[0]?.getAttribute('dominant-baseline')).toBe('central');
		// Unter dem Ring und in der Textfassung (Etappe 3b); die erste steht unter dem Ring.
		const [wert] = screen.getAllByText('12,3 GB / 31,7 GB');
		if (wert === undefined) throw new Error('Wert fehlt');
		expect(wert.closest('svg')).toBeNull();
		expect(
			screen.getByRole('img', { name: `Arbeitsspeicher: ${p} Prozent, 12,3 GB / 31,7 GB` })
		).toBeInTheDocument();
	});

	// Etappe 3a-4: Die Farbe kommt nur aus der Stufe des Servers, nicht aus der Zahl.
	it.each<[Belegung, number, string]>([
		['normal', 95, 'b-ok'],
		['hoch', 10, 'b-wartet'],
		['kritisch', 50, 'b-problem'],
		['unbekannt', 0, 'b-neutral']
	])('Stufe %s (bei %i %%): Klasse %s', (belegung, p, klasse) => {
		const { container } = render(Ring, { percent: p, belegung, label: 'Platte', value: 'x' });
		expect(container.querySelector('circle.fg')?.classList.contains(klasse)).toBe(true);
		expect(container.querySelector('.ring')?.getAttribute('data-belegung')).toBe(belegung);
	});

	it('unbekannt ist nie grün', () => {
		const { container } = render(Ring, {
			percent: 10,
			belegung: 'unbekannt',
			label: 'Platte',
			value: 'x'
		});
		expect(container.querySelector('circle.fg')?.classList.contains('b-ok')).toBe(false);
	});
});
