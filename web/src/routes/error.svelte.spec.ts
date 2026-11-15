import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

// Schritt E2: die Fehlerseite bei „Anmeldung nötig“ sagt, was gilt (ohne Anmeldung nur die
// Anmeldeseite), unter Windows dazu den Satz zu Edge, sonst den neutralen, und verlinkt die Anmeldung.
const { ort } = vi.hoisted(() => ({
	ort: {
		url: new URL('http://127.0.0.1:8788/pc?x=1'),
		status: 401,
		error: { message: 'Bitte mit dem Windows-Konto anmelden.', art: 'anmeldung' }
	}
}));
vi.mock('$app/state', () => ({ page: ort }));
vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
const { default: Fehler } = await import('./+error.svelte');

function wer(status: number): void {
	vi.stubGlobal(
		'fetch',
		vi.fn(() => Promise.resolve(new Response('{}', { status })))
	);
}

afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
	ort.error = { message: 'Bitte mit dem Windows-Konto anmelden.', art: 'anmeldung' };
});

describe('Fehlerseite „Anmeldung nötig“ (Schritt E2)', () => {
	it('Windows: der neue Satz, der Satz zu Edge und der Link zur Anmeldung mit Ziel', async () => {
		wer(200);
		render(Fehler);
		expect(
			screen.getByText('Anmeldung nötig. Ohne Anmeldung zeigt das Dashboard nur die Anmeldeseite.')
		).toBeInTheDocument();
		const edge = await screen.findByRole('link', { name: 'http://localhost:8788/' });
		expect(edge).toHaveAttribute('href', 'http://localhost:8788/pc?x=1');
		expect(screen.getByRole('link', { name: 'Zur Anmeldung' })).toHaveAttribute(
			'href',
			'http://localhost:8788/anmelden?ziel=%2Fpc%3Fx%3D1'
		);
		expect(document.body.textContent).not.toMatch(/jedem Browser|auch ohne/);
	});

	it('anderes System (501): der neutrale Satz, kein Edge', async () => {
		wer(501);
		render(Fehler);
		expect(await screen.findByText('Anmelden geht hier mit einem Passkey.')).toBeInTheDocument();
		expect(document.body.textContent).not.toContain('Edge');
		expect(screen.getByRole('link', { name: 'Zur Anmeldung' })).toBeInTheDocument();
	});

	it('Passkey (fremdes Windows-Konto) und fremdes Konto: kein Satz mehr, dass Seiten ohne Anmeldung gehen', () => {
		for (const art of ['passkey', 'fremdes-konto']) {
			ort.error = { message: 'x', art };
			render(Fehler);
			expect(document.body.textContent).not.toMatch(/auch ohne|jedem Browser/);
			expect(screen.getByRole('link', { name: /Zur Anmeldung/ })).toHaveAttribute(
				'href',
				'http://localhost:8788/anmelden?ziel=%2Fpc%3Fx%3D1'
			);
			cleanup();
		}
	});
});
