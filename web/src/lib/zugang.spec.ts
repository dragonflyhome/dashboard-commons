import { afterEach, describe, expect, it, vi } from 'vitest';
import { FRIST_MS } from './rueckfall.ts';
import { ZUGANG_ADRESSE, Zugang, antwortOhneZugang, mitZugang } from './zugang.ts';
import { ladeAnstehend } from './anstehend.ts';
import { ladeAnzahl } from './aufgaben.ts';

function antwort(status: number, json?: unknown) {
	return vi.fn(() =>
		Promise.resolve(
			new Response(json === undefined ? null : JSON.stringify(json), {
				status,
				headers: { 'content-type': 'application/json' }
			})
		)
	);
}

afterEach(() => {
	vi.useRealTimers();
});

describe('Zugang (Etappe 3b: weniger 401 ohne Anmeldung)', () => {
	it('fragt einmal; 200 da, 401 fehlt, 403 fremdes-konto fremd, sonst unbekannt', async () => {
		for (const [f, soll] of [
			[antwort(200, {}), 'da'],
			[antwort(401, { art: 'anmeldung' }), 'fehlt'],
			[antwort(403, { art: 'fremdes-konto' }), 'fremd'],
			[antwort(403, { art: 'herkunft' }), 'unbekannt'],
			[antwort(503), 'unbekannt'],
			[vi.fn(() => Promise.reject(new TypeError('Failed to fetch'))), 'unbekannt']
		] as const) {
			const z = new Zugang();
			expect(await z.pruefen(f as unknown as typeof fetch)).toBe(soll);
			expect(z.stand).toBe(soll);
			expect(f).toHaveBeenCalledTimes(1);
			expect((f.mock.calls[0] as unknown[])[0]).toBe(ZUGANG_ADRESSE);
		}
	});

	it('gleichzeitige Fragen teilen sich eine Anfrage', async () => {
		const z = new Zugang();
		const f = antwort(401);
		const [a, b] = await Promise.all([
			z.pruefen(f as unknown as typeof fetch),
			z.pruefen(f as unknown as typeof fetch)
		]);
		expect([a, b]).toEqual(['fehlt', 'fehlt']);
		expect(f).toHaveBeenCalledTimes(1);
	});

	it('ohne Anmeldung gehen geschützte Abfragen nicht ins Netz und zeigen denselben Satz', async () => {
		const z = new Zugang();
		z.setzen('fehlt');
		const netz = vi.fn(() => Promise.resolve(new Response('{}', { status: 200 })));
		const f = mitZugang(netz, z);
		const l = await ladeAnstehend(f);
		expect(l.stand).toBe('ohne-anmeldung');
		expect(await ladeAnzahl(f)).toBeNull();
		expect(netz).not.toHaveBeenCalled();
		z.setzen('fremd');
		expect((await ladeAnstehend(f)).stand).toBe('fremdes-konto');
		expect(netz).not.toHaveBeenCalled();
	});

	it('mit Anmeldung oder unbekannt geht alles ins Netz', async () => {
		for (const stand of ['da', 'unbekannt'] as const) {
			const z = new Zugang();
			z.setzen(stand);
			const netz = antwort(200, { offen: 2 });
			expect(await ladeAnzahl(mitZugang(netz as unknown as typeof fetch, z))).toBe(2);
			expect(netz).toHaveBeenCalledTimes(1);
		}
	});

	it('fehlt verfällt wie der Rückfall nach FRIST_MS; danach wieder echt', () => {
		vi.useFakeTimers();
		const z = new Zugang();
		z.setzen('fehlt');
		expect(z.gesperrt).toBe(true);
		vi.advanceTimersByTime(FRIST_MS - 1);
		expect(z.gesperrt).toBe(true);
		vi.advanceTimersByTime(1);
		expect(z.stand).toBe('unbekannt');
		expect(z.gesperrt).toBe(false);
	});

	it('die Antwort ohne Zugang hat Code, Satz und art wie der Server', async () => {
		const r = antwortOhneZugang('fehlt');
		expect(r.status).toBe(401);
		expect(await r.json()).toEqual({
			fehler: 'Bitte mit dem Windows-Konto anmelden.',
			art: 'anmeldung'
		});
		const f = antwortOhneZugang('fremd');
		expect(f.status).toBe(403);
		expect(((await f.json()) as { art: string }).art).toBe('fremdes-konto');
	});
});
