import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type * as PasskeyModul from '#lib/passkey.ts';
import { schlossZiel } from '#lib/verbindung.ts';
import type { PasskeyStand } from '$vertrag';

// Schritt E2: Ohne Anmeldung leitet jede Seite nach /anmelden?ziel=<pfad>&umgeleitet=1. Die Seite
// fragt /api/passkey/stand (gilt schon eine Anmeldung, geht es nach so einer Umleitung sofort weiter) und
// /api/anmeldung/wer (Windows: der Satz zu Edge; sonst der Satz zum Passkey).
const { ort, passkey } = vi.hoisted(() => ({
	ort: { url: new URL('http://localhost:8787/anmelden') },
	passkey: { stand: vi.fn(), eigeneAntraege: vi.fn(), weiter: vi.fn() }
}));
vi.mock('$app/state', () => ({ page: ort }));
vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
vi.mock('#lib/freikanal.svelte.ts', () => ({
	freierKanal: { zustand: 'nicht_angemeldet', abgemeldet: vi.fn() }
}));
vi.mock('#lib/passkey.ts', async (original) => ({
	...(await original<typeof PasskeyModul>()),
	stand: passkey.stand,
	eigeneAntraege: passkey.eigeneAntraege,
	weiter: passkey.weiter
}));
const { default: Page } = await import('./+page.svelte');

const KEINE: PasskeyStand = {
	wie: 'keine',
	passkey: null,
	sitzung_bis: null,
	frisch_bis: null,
	verfuegbar: true
};
const MIT_PASSKEY: PasskeyStand = {
	...KEINE,
	wie: 'passkey',
	passkey: 'probekonto',
	sitzung_bis: '2026-11-15T18:00:00+01:00'
};

/** `/api/anmeldung/wer` antwortet mit `status`; jede andere Adresse ist ein Fehler im Test. */
function wer(status: number) {
	const abruf = vi.fn((pfad: string) => {
		if (pfad !== '/api/anmeldung/wer') throw new Error(`unerwartet: ${pfad}`);
		return Promise.resolve(new Response('{}', { status }));
	});
	vi.stubGlobal('fetch', abruf);
	return abruf;
}

beforeEach(() => {
	passkey.eigeneAntraege.mockRejectedValue(new Error('kein Konto'));
	passkey.stand.mockResolvedValue(KEINE);
	passkey.weiter.mockReset();
	ort.url = new URL('http://localhost:8787/anmelden');
});
afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

describe('/anmelden (Schritt E2)', () => {
	// Unter Windows folgt Edge dem Link „Zur Anmeldung“ der 401-Seite und kommt als Hauptkonto an.
	it.each([
		['Passkey', MIT_PASSKEY],
		['Windows (Hauptkonto)', { ...KEINE, wie: 'hauptkonto' } satisfies PasskeyStand]
	])('umgeleitet, mit gültiger Anmeldung (%s): geht sofort weiter zum Ziel', async (_, gilt) => {
		wer(200);
		passkey.stand.mockResolvedValue(gilt);
		ort.url = new URL(
			'http://localhost:8787/anmelden?ziel=%2Fprojects%2Fbeispiel%3Fx%3D1&umgeleitet=1'
		);
		render(Page);
		await vi.waitFor(() => {
			expect(passkey.weiter).toHaveBeenCalledWith('/projects/beispiel?x=1');
		});
		expect(passkey.stand).toHaveBeenCalled();
	});

	// Das Schloss trägt den Rückweg (?ziel=), aber nie die Markierung: Die Seite bleibt zum Anlegen.
	it.each([
		['Passkey', MIT_PASSKEY],
		['Windows (Hauptkonto)', { ...KEINE, wie: 'hauptkonto' } satisfies PasskeyStand]
	])(
		'angemeldet (%s) über das Schloss: bleibt, Passkey anlegen, Weiter zum Ziel',
		async (_, gilt) => {
			wer(200);
			passkey.stand.mockResolvedValue(gilt);
			// Wie ein echter Sprung zum Ziel: Die Seite ist danach weg, der Antrag unten nicht mehr da.
			passkey.weiter.mockImplementation(() => {
				cleanup();
			});
			const ziel = schlossZiel('angemeldet', '/anstehend?stufe=jetzt');
			ort.url = new URL(ziel, 'http://localhost:8787');
			render(Page);
			const weiter = await screen.findByRole('link', { name: 'Weiter' });
			expect(weiter).toHaveAttribute('href', '/anstehend?stufe=jetzt');
			// Das Laden beim Öffnen ist durch (stand geholt, Entscheidung gefallen), der Antrag steht noch da.
			await vi.waitFor(() => {
				expect(passkey.stand).toHaveBeenCalled();
			});
			await new Promise((fertig) => setTimeout(fertig, 0));
			// toBeInTheDocument statt not.toBeNull: `?.` ergäbe ohne Seite undefined und ginge durch.
			expect(document.querySelector('#passkey-anlegen #geraet')).toBeInTheDocument();
			expect(passkey.weiter).not.toHaveBeenCalled();
		}
	);

	it.each(['https://fremd.example/', '//fremd.example/x', '/anmelden?ziel=%2Fpc&umgeleitet=1'])(
		'umgeleitet mit fremdem Ziel %s: nur zur Startseite (zielPruefen), keine offene Umleitung',
		async (fremd) => {
			wer(200);
			passkey.stand.mockResolvedValue(MIT_PASSKEY);
			ort.url = new URL(
				`http://localhost:8787/anmelden?ziel=${encodeURIComponent(fremd)}&umgeleitet=1`
			);
			render(Page);
			await vi.waitFor(() => {
				expect(passkey.weiter).toHaveBeenCalledWith('/');
			});
		}
	);

	it('mit gültigem Passkey ohne Ziel (Schloss oben): bleibt zum Verwalten und Abmelden', async () => {
		wer(200);
		passkey.stand.mockResolvedValue(MIT_PASSKEY);
		render(Page);
		expect(await screen.findByRole('button', { name: 'Abmelden' })).toBeInTheDocument();
		expect(passkey.weiter).not.toHaveBeenCalled();
	});

	it('umgeleitet ohne Anmeldung: kein Weiter, der Satz zur Anmeldung', async () => {
		wer(501);
		ort.url = new URL('http://localhost:8787/anmelden?ziel=%2Fpc&umgeleitet=1');
		render(Page);
		expect(
			await screen.findByText(
				'Anmeldung nötig. Ohne Anmeldung zeigt das Dashboard nur die Anmeldeseite.'
			)
		).toBeInTheDocument();
		await vi.waitFor(() => {
			expect(passkey.stand).toHaveBeenCalled();
		});
		expect(passkey.weiter).not.toHaveBeenCalled();
	});

	it('Windows (wer 200): der Satz zu Edge mit der Adresse über localhost, Link aufs Ziel', async () => {
		const abruf = wer(200);
		ort.url = new URL('http://127.0.0.1:8788/anmelden?ziel=%2Fpc');
		render(Page);
		const link = await screen.findByRole('link', { name: 'http://localhost:8788/' });
		expect(link).toHaveAttribute('href', 'http://localhost:8788/pc');
		expect(link.closest('p')?.textContent).toMatch(/^\s*Mit deiner Windows-Anmeldung .*Edge/);
		expect(screen.queryByText('Anmelden geht hier mit einem Passkey.')).toBeNull();
		expect(abruf).toHaveBeenCalledWith('/api/anmeldung/wer', expect.anything());
	});

	it('anderes System (wer 501): der neutrale Satz zum Passkey, kein Edge', async () => {
		wer(501);
		render(Page);
		expect(await screen.findByText('Anmelden geht hier mit einem Passkey.')).toBeInTheDocument();
		expect(document.body.textContent).not.toContain('Edge');
	});

	it('kein Server (wer wirft): weder der eine noch der andere Satz', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(() => Promise.reject(new TypeError('Failed to fetch')))
		);
		render(Page);
		await vi.waitFor(() => {
			expect(passkey.stand).toHaveBeenCalled();
		});
		expect(screen.queryByText('Anmelden geht hier mit einem Passkey.')).toBeNull();
		expect(document.body.textContent).not.toContain('Windows-Anmeldung öffnet');
	});
});
