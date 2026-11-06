/**
 * Logo-Entwürfe (Etappe 3b): drei Entwürfe, je hell und dunkel; die SVG
 * halten die Asset-Regeln des Zielbilds (kein Skript, keine Verweise nach
 * außen, kein foreignObject, Farben nur über Token oder currentColor).
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

vi.mock('$app/state', () => ({
	page: {
		url: new URL('http://localhost:8787/katalog/entwuerfe/logo'),
		params: {},
		route: { id: '/katalog/entwuerfe/logo' }
	}
}));

const { default: Seite } = await import('./+page.svelte');

const ENTWUERFE = ['LogoBluete.svelte', 'LogoLinie.svelte', 'LogoKreis.svelte'];

describe('Logo-Entwürfe (Etappe 3b)', () => {
	it('drei Entwürfe, jeder hell und dunkel nebeneinander', () => {
		render(Seite);
		const figuren = screen.getAllByRole('figure');
		expect(figuren).toHaveLength(3);
		for (const f of figuren) {
			const tafeln = [...f.querySelectorAll('[data-thema-vorschau]')].map((t) =>
				t.getAttribute('data-thema-vorschau')
			);
			expect(tafeln).toEqual(['hell', 'dunkel']);
			// Groß mit Namen für Vorlese-Programme; die kleine Fassung ist Schmuck.
			expect(within(f).getAllByRole('img').length).toBe(2);
		}
		expect(screen.getByRole('img', { name: 'Gewähltes Logo: Blüte' })).toBeInTheDocument();
	});

	it('nennt die Wahl: Entwurf 1 „Blüte“ ist gewählt, die anderen nicht', () => {
		render(Seite);
		expect(document.body.textContent).toContain('Gewählt ist Entwurf 1 „Blüte“');
		const titel = screen
			.getAllByRole('figure')
			.map((f) =>
				within(f).getByRole('heading', { level: 2 }).textContent.replace(/\s+/g, ' ').trim()
			);
		expect(titel).toEqual(['Entwurf 1: Blüte Gewählt', 'Entwurf 2: Linie', 'Entwurf 3: Kreis']);
	});

	it.each(ENTWUERFE)('%s hält die Asset-Regeln', (datei) => {
		const text = readFileSync(join(import.meta.dirname, datei), 'utf8');
		const markup = text.replace(/<style>[\s\S]*<\/style>/, '');
		expect(text).not.toMatch(/<script/i);
		expect(markup).not.toMatch(/\bhref\b|https?:|url\(|<foreignObject|<image|<use\b|\bon[a-z]+=/i);
		// Farben nur im Stil über Klassen, nie als Attribut.
		expect(markup).not.toMatch(/\b(?:fill|stroke|color|style)=/);
		const stil = /<style>([\s\S]*)<\/style>/.exec(text)?.[1] ?? '';
		const farben = [...stil.matchAll(/\b(?:fill|stroke|color):\s*([^;]+);/g)].map((m) => m[1]);
		expect(farben.length).toBeGreaterThan(1);
		for (const f of farben) expect(f).toMatch(/^(?:var\(--[a-z0-9-]+\)|currentColor|none)$/);
		expect(markup).toMatch(/<svg[^>]*role="img"[^>]*aria-label="Logo-Entwurf /);
	});
});
