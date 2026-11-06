import { render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { KeyList } from '$vertrag';

// Nach dem Speichern leitet der Server auf ?gespeichert=<name> um. Die Seite
// feiert einmal und nimmt den Parameter danach aus der Adresse (Auflage
// Hotfix 3a-1), damit ein Neuladen nicht noch einmal feiert.
const { ort, goto, nachNavigation } = vi.hoisted(() => ({
	ort: { url: new URL('http://localhost:8787/schluessel') },
	goto: vi.fn(() => Promise.resolve()),
	nachNavigation: [] as ((nav: { to: { url: URL } | null }) => void)[]
}));
vi.mock('$app/state', () => ({ page: ort }));
vi.mock('$app/navigation', () => ({
	goto,
	afterNavigate: (f: (nav: { to: { url: URL } | null }) => void) => {
		nachNavigation.push(f);
	}
}));
vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));

const { default: Page } = await import('./+page.svelte');

const daten: KeyList = {
	keys: [
		{
			name: 'beispiel-token',
			zweck: 'Zum Testen',
			hinweis: '',
			angefordert_von: 'chat:test',
			target: 'dashboard:beispiel-token',
			saved_at: '2026-10-30T10:00:00+01:00',
			stand: 'gespeichert'
		}
	],
	error: null
};

/** Seite laden, wie SvelteKit es täte: erst zeichnen, dann afterNavigate. */
function laden(adresse: string) {
	ort.url = new URL(adresse);
	nachNavigation.length = 0;
	vi.stubGlobal(
		'matchMedia',
		vi.fn(() => ({ matches: false }))
	);
	vi.stubGlobal(
		'fetch',
		vi.fn(() => Promise.resolve(new Response('{}', { status: 404 })))
	);
	const r = render(Page, { data: daten, params: {} } as never);
	navigiert(ort.url);
	return r;
}

/** afterNavigate mit der Adresse der Navigation aufrufen, wie SvelteKit. */
function navigiert(url: URL) {
	for (const f of nachNavigation) f({ to: { url } });
}

afterEach(() => {
	goto.mockClear();
	vi.unstubAllGlobals();
});

describe('/schluessel', () => {
	it('direkt nach dem Speichern: feiert und nimmt ?gespeichert aus der Adresse', async () => {
		const { container } = laden('http://localhost:8787/schluessel?gespeichert=beispiel-token');
		expect(screen.getByText(/„beispiel-token“ ist gespeichert/)).toBeInTheDocument();
		expect(container.querySelector('.confetti')).not.toBeNull();
		await waitFor(() => {
			expect(goto).toHaveBeenCalledWith('/schluessel', { shallow: true, replace: true });
		});
	});

	it('goto mit shallow ruft afterNavigate erneut, page.url behält den Parameter: nur ein goto', async () => {
		laden('http://localhost:8787/schluessel?gespeichert=beispiel-token');
		await waitFor(() => {
			expect(goto).toHaveBeenCalledTimes(1);
		});
		// So sah die Endlosschleife aus: SvelteKit meldet die flache
		// Navigation, page.url hat den Parameter noch.
		navigiert(new URL('http://localhost:8787/schluessel'));
		navigiert(ort.url);
		navigiert(ort.url);
		expect(goto).toHaveBeenCalledTimes(1);
	});

	it('ohne ?gespeichert (etwa neu geladen): kein Konfetti, kein goto', () => {
		const { container } = laden('http://localhost:8787/schluessel');
		expect(container.querySelector('.confetti')).toBeNull();
		expect(screen.queryByText(/ist gespeichert\./)).toBeNull();
		expect(goto).not.toHaveBeenCalled();
	});

	it('Etappe 3a+: ein Tresor-Fehler des Boten heißt „Stand unbekannt“, nicht „fehlt“', () => {
		const roh = daten.keys[0];
		if (roh === undefined) throw new Error('Beispiel fehlt');
		ort.url = new URL('http://localhost:8787/schluessel');
		vi.stubGlobal(
			'matchMedia',
			vi.fn(() => ({ matches: false }))
		);
		vi.stubGlobal(
			'fetch',
			vi.fn(() => Promise.resolve(new Response('{}', { status: 404 })))
		);
		const anders: KeyList = {
			keys: [
				{ ...roh, saved_at: null, stand: 'unbekannt' },
				{ ...roh, name: 'zweiter', saved_at: null, stand: 'fehlt' }
			],
			error: null
		};
		render(Page, { data: anders, params: {} } as never);
		expect(screen.getByText('Stand unbekannt')).toBeInTheDocument();
		expect(screen.getAllByText('fehlt')).toHaveLength(1);
		expect(screen.getByText(/2 angefordert, 1 fehlt,\s*1 unbekannt\./)).toBeInTheDocument();
	});
});
