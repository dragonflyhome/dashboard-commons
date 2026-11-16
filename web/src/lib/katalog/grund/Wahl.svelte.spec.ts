/**
 * Etappe S2: die Karten-Form von `Wahl`. Ohne Snippet bleibt der Baustein,
 * wie er war (Segmente); mit `karte` zeichnet jede Möglichkeit ihren Inhalt,
 * und Name und Beschreibung eines Optionsfelds kommen nur von den zwei
 * Elementen, die der Inhalt mit den übergebenen Kennungen versieht.
 */
import { fireEvent, render, screen } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import { describe, expect, it } from 'vitest';
import Wahl from './Wahl.svelte';
import type { Option } from './typen.ts';

const OPTIONEN: readonly Option[] = [
	{ wert: 'a', text: 'Erste' },
	{ wert: 'b', text: 'Zweite' },
	{ wert: 'c', text: 'Dritte', gesperrt: true }
];

const karte = createRawSnippet<[Option, { name: string; beschreibung: string }]>((o, ids) => ({
	render: () =>
		`<span><span id="${ids().name}">${o().text}</span><span aria-hidden="true">Muster</span><span id="${ids().beschreibung}">Erklärung zu ${o().text}</span></span>`
}));

describe('Wahl ohne Snippet: Segmente wie bisher', () => {
	it('Optionsfelder mit ihrem Text als Namen, in einer benannten Gruppe', () => {
		render(Wahl, { beschriftung: 'Ansicht', optionen: OPTIONEN, wert: 'b' });
		expect(screen.getByRole('radiogroup', { name: 'Ansicht' })).toBeInTheDocument();
		expect(screen.getAllByRole('radio').map((f) => f.textContent.trim())).toEqual([
			'Erste',
			'Zweite',
			'Dritte'
		]);
		expect(screen.getByRole('radio', { name: 'Zweite' })).toHaveAttribute('aria-checked', 'true');
		expect(document.querySelector('.segmente')).not.toBeNull();
		expect(document.querySelector('.karten')).toBeNull();
		expect(screen.getByRole('radio', { name: 'Erste' })).not.toHaveAttribute('aria-labelledby');
	});
});

describe('Wahl mit Snippet: Karten', () => {
	it('Name nur aus dem Namen der Karte, Beschreibung vorhanden', () => {
		render(Wahl, { beschriftung: 'Farben', optionen: OPTIONEN, wert: 'a', karte });
		expect(screen.getByRole('radiogroup', { name: 'Farben' })).toBeInTheDocument();
		expect(document.querySelector('.karten')).not.toBeNull();
		expect(document.querySelector('.segmente')).toBeNull();
		for (const o of OPTIONEN) {
			const feld = screen.getByRole('radio', { name: o.text });
			// Nicht der ganze Inhalt („Muster“, Erklärung, Haken), nur der Name.
			expect(feld).toHaveAccessibleName(o.text);
			expect(feld).toHaveAccessibleDescription(`Erklärung zu ${o.text}`);
			expect(feld.textContent).toContain('Muster');
		}
	});

	it('die gewählte Karte meldet aria-checked; der Haken ist nur Schmuck', async () => {
		render(Wahl, { beschriftung: 'Farben', optionen: OPTIONEN, wert: 'a', karte });
		const erste = screen.getByRole('radio', { name: 'Erste' });
		const zweite = screen.getByRole('radio', { name: 'Zweite' });
		expect(erste).toHaveAttribute('aria-checked', 'true');
		expect(zweite).toHaveAttribute('aria-checked', 'false');
		for (const feld of [erste, zweite]) {
			expect(feld.querySelector('.haken')).toHaveAttribute('aria-hidden', 'true');
		}
		await fireEvent.click(zweite);
		expect(zweite).toHaveAttribute('aria-checked', 'true');
		expect(erste).toHaveAttribute('aria-checked', 'false');
	});

	it('eine gesperrte Karte ist nicht wählbar', () => {
		render(Wahl, { beschriftung: 'Farben', optionen: OPTIONEN, wert: 'a', karte });
		expect(screen.getByRole('radio', { name: 'Dritte' })).toBeDisabled();
	});

	it('jede Karte hat eigene Kennungen für Name und Beschreibung', () => {
		render(Wahl, { beschriftung: 'Farben', optionen: OPTIONEN, wert: 'a', karte });
		const felder = screen.getAllByRole('radio');
		const ids = felder.flatMap((f) => [
			f.getAttribute('aria-labelledby'),
			f.getAttribute('aria-describedby')
		]);
		expect(new Set(ids).size).toBe(ids.length);
		for (const id of ids) expect(document.getElementById(id ?? '')).not.toBeNull();
	});
});
