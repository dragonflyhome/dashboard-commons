/**
 * Gesperrte Knöpfe, gerendert (Etappe B2, Auflage der Architektur): der
 * Hauptknopf, Wahl und Schalter mit den Stilen aus app.css. Gesperrt heißt:
 * gestrichelter Rand, keine Deckkraft, cursor not-allowed; der Hauptknopf hat
 * dazu Rand und Text in Textfarbe auf abgeschwächter Fläche, und Hover gilt
 * für gesperrte Knöpfe nicht.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { cleanup, render, screen } from '@testing-library/svelte';
import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { Knopf, Schalter, Wahl } from '$katalog';
import { createRawSnippet } from 'svelte';

const CSS = readFileSync(join(import.meta.dirname, 'app.css'), 'utf8');
let stil: HTMLStyleElement;
beforeAll(() => {
	stil = document.createElement('style');
	stil.textContent = CSS;
	document.head.append(stil);
});
afterAll(() => {
	stil.remove();
});
afterEach(cleanup);

const text = (t: string) => createRawSnippet(() => ({ render: () => `<span>${t}</span>` }));

describe('Gesperrt, gerendert', () => {
	it('Hauptknopf: gestrichelter Rand in Textfarbe, Fläche gemischt, Text in --text, keine Deckkraft', () => {
		render(Knopf, { art: 'haupt', gesperrt: true, children: text('Speichern') });
		const k = screen.getByRole('button', { name: 'Speichern' });
		expect(k).toBeDisabled();
		const s = getComputedStyle(k);
		expect(s.borderStyle).toBe('dashed');
		expect(s.cursor).toBe('not-allowed');
		expect(s.opacity === '' || s.opacity === '1').toBe(true);
		// Farben aus Token rechnet jsdom nicht aus: die Regel trifft den Knopf, und sie setzt Rand, Fläche und Text.
		expect(k.matches('.button.primary:disabled')).toBe(true);
		const regel = /\.button\.primary:disabled,[^{]*\{([^}]*)\}/.exec(CSS)?.[1] ?? '';
		expect(regel).toContain('border-color: var(--text);');
		expect(regel).toContain('background: color-mix(in srgb, var(--accent) 25%, var(--card));');
		expect(regel).toContain('color: var(--text);');
		expect(regel).not.toContain('opacity');
	});

	it('Hover gilt für gesperrte Knöpfe nicht (Regel in app.css)', () => {
		const css = CSS;
		expect(css).toContain(".button:hover:not(:disabled, [aria-disabled='true'])");
		expect(css).toContain(".button:active:not(:disabled, [aria-disabled='true'])");
		expect(css).not.toMatch(/\.button:hover\s*\{/);
	});

	it('Wahl: gesperrtes Segment mit cursor not-allowed und ohne Deckkraft', () => {
		render(Wahl, {
			beschriftung: 'Ansicht',
			optionen: [
				{ wert: 'a', text: 'Kacheln' },
				{ wert: 'b', text: 'Spalten', gesperrt: true }
			]
		});
		const seg = screen.getByRole('radio', { name: 'Spalten' });
		expect(seg).toBeDisabled();
		const s = getComputedStyle(seg);
		expect(s.cursor).toBe('not-allowed');
		expect(s.opacity === '' || s.opacity === '1').toBe(true);
	});

	it('Schalter: gesperrt mit gestricheltem Rand, cursor not-allowed, ohne Deckkraft', () => {
		render(Schalter, { beschriftung: 'Nur offene', gesperrt: true });
		const sw = screen.getByRole('switch', { name: 'Nur offene' });
		expect(sw).toBeDisabled();
		const s = getComputedStyle(sw);
		expect(s.borderStyle).toBe('dashed');
		expect(s.cursor).toBe('not-allowed');
		expect(s.opacity === '' || s.opacity === '1').toBe(true);
	});
});
