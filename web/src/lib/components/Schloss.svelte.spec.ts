import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { LiveZustand } from '#lib/verbindung.ts';

// Kopfleiste, Fassung 2: Live-Punkt als Link zum Stand und Schloss.
vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
vi.mock('$app/state', () => ({
	page: { url: new URL('http://localhost:8787/anstehend?stufe=jetzt') }
}));
const { default: LiveBadge } = await import('./LiveBadge.svelte');
const { default: Schloss } = await import('./Schloss.svelte');

const ALLE: [LiveZustand, string][] = [
	['angemeldet', 'live, angemeldet'],
	['nicht_angemeldet', 'live, nicht angemeldet'],
	['nicht_registriert', 'live, nicht registriert'],
	// Ohne Verbindung Zustand 1 statt „Verbindung nicht gefunden“ / „unterbrochen“.
	['nicht_gefunden', 'Keine Verbindung zum Dienst, Daten können veraltet sein'],
	['unterbrochen', 'Keine Verbindung zum Dienst, Daten können veraltet sein']
];

describe('Live-Punkt', () => {
	it.each(ALLE)('%s: Text, Name und Link zum Abschnitt „Dashboard“ auf /pc', (zustand, text) => {
		const { unmount } = render(LiveBadge, { zustand });
		const link = screen.getByRole('link');
		expect(link).toHaveAttribute('href', '/pc#dashboard');
		expect(link).toHaveTextContent(text);
		expect(link).toHaveAccessibleName(
			zustand.startsWith('nicht_g') || zustand === 'unterbrochen'
				? `${text}, Stand ansehen`
				: `Verbindung: ${text}, Stand ansehen`
		);
		expect(link).toHaveAttribute('data-zustand', zustand);
		unmount();
	});
	it('solange der erste Versuch läuft: nur der Punkt, kein geratener Zustand', () => {
		render(LiveBadge, { zustand: null });
		const link = screen.getByRole('link');
		expect(link.textContent.trim()).toBe('');
		expect(link).toHaveAccessibleName('Verbindung wird geprüft, Stand ansehen');
	});
});

describe('Schloss', () => {
	it('angemeldet: offen, führt zum Anlegen eines Passkeys, mit Rückweg', () => {
		render(Schloss, { zustand: 'angemeldet' });
		const s = screen.getByRole('link', { name: 'Angemeldet, Passkey verwalten' });
		expect(s).toHaveAttribute('data-schloss', 'offen');
		expect(s).toHaveClass('offen');
		expect(s).toHaveAttribute(
			'href',
			'/anmelden?ziel=%2Fanstehend%3Fstufe%3Djetzt#passkey-anlegen'
		);
	});
	it('nicht angemeldet: zu, führt zum Anmelden mit Passkey', () => {
		render(Schloss, { zustand: 'nicht_angemeldet' });
		const s = screen.getByRole('link', { name: 'Nicht angemeldet, mit Passkey anmelden' });
		expect(s).toHaveAttribute('data-schloss', 'zu');
		expect(s.getAttribute('href')).toMatch(/^\/anmelden\?ziel=.*#mit-passkey-anmelden$/);
	});
	it('nicht registriert: zu, führt zum Passkey-Antrag', () => {
		render(Schloss, { zustand: 'nicht_registriert' });
		const s = screen.getByRole('link', { name: 'Nicht registriert, Passkey beantragen' });
		expect(s).toHaveAttribute('data-schloss', 'zu');
		expect(s.getAttribute('href')).toMatch(/#passkey-anlegen$/);
	});
	it.each(['nicht_gefunden', 'unterbrochen', null] as const)('%s: zu, Ziel Anmelden', (zustand) => {
		const { unmount } = render(Schloss, { zustand });
		const s = screen.getByRole('link');
		expect(s).toHaveAttribute('data-schloss', 'zu');
		expect(s.getAttribute('href')).toMatch(/#mit-passkey-anmelden$/);
		unmount();
	});
	it('das Zeichen ist Schmuck; der Name steht am Link', () => {
		render(Schloss, { zustand: 'angemeldet' });
		expect(screen.getByRole('link').querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
	});
});
