import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	FRIST_MS,
	ZUGANG_ADRESSE,
	Zugang,
	antwortOhneZugang,
	beiRueckkehrAuffrischen,
	mitZugang
} from './zugang.ts';
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

	it('fehlt verfällt nach FRIST_MS; danach wieder echt', () => {
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

	it('Schritt E2b: auffrischen fragt nur bei fehlt oder fremd neu, sonst bleibt der Stand', async () => {
		for (const stand of ['fehlt', 'fremd'] as const) {
			const z = new Zugang();
			z.setzen(stand);
			const f = antwort(200, {});
			expect(await z.auffrischen(f as unknown as typeof fetch)).toBe('da');
			expect(f).toHaveBeenCalledTimes(1);
			expect(z.gesperrt).toBe(false);
		}
		for (const stand of ['da', 'unbekannt'] as const) {
			const z = new Zugang();
			z.setzen(stand);
			const f = antwort(401);
			expect(await z.auffrischen(f as unknown as typeof fetch)).toBe(stand);
			expect(f).not.toHaveBeenCalled();
		}
	});

	it('Schritt E2b: zurück im Tab (sichtbar oder Fokus) gilt ein gemerktes fehlt nicht weiter', () => {
		const doc = Object.assign(new EventTarget(), { visibilityState: 'hidden' });
		const w = Object.assign(new EventTarget(), { document: doc }) as unknown as Window;
		const z = new Zugang();
		const auffrischen = vi.spyOn(z, 'auffrischen').mockResolvedValue('da');
		const ab = beiRueckkehrAuffrischen(z, w);
		// Unsichtbar (in den anderen Tab gewechselt): nichts.
		doc.dispatchEvent(new Event('visibilitychange'));
		expect(auffrischen).not.toHaveBeenCalled();
		doc.visibilityState = 'visible';
		doc.dispatchEvent(new Event('visibilitychange'));
		w.dispatchEvent(new Event('focus'));
		expect(auffrischen).toHaveBeenCalledTimes(2);
		ab();
		w.dispatchEvent(new Event('focus'));
		expect(auffrischen).toHaveBeenCalledTimes(2);
	});

	it('Schritt E2b: nach der Anmeldung im anderen Tab geht die nächste geschützte Abfrage ins Netz', async () => {
		const z = new Zugang();
		z.setzen('fehlt');
		const doc = Object.assign(new EventTarget(), { visibilityState: 'visible' });
		const w = Object.assign(new EventTarget(), { document: doc }) as unknown as Window;
		const ab = beiRueckkehrAuffrischen(z, w);
		const netz = antwort(200, { offen: 3 });
		const f = mitZugang(netz, z);
		expect(await ladeAnzahl(f)).toBeNull();
		expect(netz).not.toHaveBeenCalled();
		w.dispatchEvent(new Event('focus'));
		// Der Stand ist vergessen, noch bevor die neue Antwort da ist.
		expect(z.gesperrt).toBe(false);
		expect(await ladeAnzahl(f)).toBe(3);
		ab();
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
