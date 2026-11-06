import { describe, expect, it } from 'vitest';
import { localhostAdresse } from './anmeldung.ts';
import {
	AnmeldungNoetig,
	ApiError,
	SATZ_KEINE_VERBINDUNG,
	fehlerText,
	getJson,
	readError,
	statusSatz
} from './api/client.ts';

describe('localhostAdresse', () => {
	it('ersetzt nur den Host, Port, Pfad, Suche und Anker bleiben', () => {
		expect(localhostAdresse('http://127.0.0.1:8787/schluessel')).toBe(
			'http://localhost:8787/schluessel'
		);
		expect(localhostAdresse('http://127.0.0.1:8788/vorgaenge?x=1#oben')).toBe(
			'http://localhost:8788/vorgaenge?x=1#oben'
		);
		expect(localhostAdresse('http://[::1]:8787/daten')).toBe('http://localhost:8787/daten');
		expect(localhostAdresse('kaputt')).toBe('http://localhost:8787/');
	});
});

function antwort(status: number, body: unknown): typeof fetch {
	return () =>
		Promise.resolve(
			new Response(JSON.stringify(body), {
				status,
				headers: { 'content-type': 'application/json' }
			})
		);
}

describe('Fehler des Servers', () => {
	it('401 mit art anmeldung wird AnmeldungNoetig', async () => {
		const f = antwort(401, { fehler: 'Bitte mit dem Windows-Konto anmelden.', art: 'anmeldung' });
		const e = await getJson('/api/schluessel', f).catch((x: unknown) => x);
		expect(e).toBeInstanceOf(AnmeldungNoetig);
		expect(e).toBeInstanceOf(ApiError);
		expect((e as ApiError).status).toBe(401);
		expect((e as ApiError).message).toBe('Bitte mit dem Windows-Konto anmelden.');
	});
	it('403 fremdes Konto behält art, andere 401 nicht als Anmeldung', async () => {
		const e = await getJson(
			'/api/daten',
			antwort(403, { fehler: 'Dieses Windows-Konto darf hier nicht.', art: 'fremdes-konto' })
		).catch((x: unknown) => x);
		expect(e).not.toBeInstanceOf(AnmeldungNoetig);
		expect((e as ApiError).art).toBe('fremdes-konto');
		const ohneArt = await getJson('/api/daten', antwort(401, { fehler: 'x' })).catch(
			(x: unknown) => x
		);
		expect(ohneArt).not.toBeInstanceOf(AnmeldungNoetig);
		expect((ohneArt as ApiError).art).toBeNull();
	});
});

describe('Fehler ohne Satz des Servers', () => {
	it('ohne JSON-Körper ein deutscher Satz statt „413 Payload Too Large“', async () => {
		const faelle: [number, string][] = [
			[401, 'Bitte mit dem Windows-Konto anmelden.'],
			[403, 'Das ist von hier aus nicht erlaubt.'],
			[404, 'Das gibt es nicht (mehr).'],
			[409, 'Das hat sich inzwischen geändert.'],
			[413, 'Das ist zu lang zum Senden.'],
			[422, 'Das ließ sich nicht verarbeiten. Bitte die Eingabe prüfen.'],
			[429, 'Gerade kommen zu viele Anfragen. Bitte kurz warten und noch einmal versuchen.'],
			[500, 'Im Dashboard ist ein Fehler passiert. Bitte später noch einmal.'],
			[502, 'Das Dashboard ist gerade nicht erreichbar. Bitte später noch einmal.'],
			[503, 'Das Dashboard ist gerade nicht erreichbar. Bitte später noch einmal.'],
			[418, 'Etwas ist schiefgegangen (Fehler 418).']
		];
		for (const [status, satz] of faelle) {
			const roh = new Response('Payload Too Large', {
				status,
				statusText: 'Payload Too Large',
				headers: { 'content-type': 'text/plain' }
			});
			const e = await readError(roh);
			expect(e.message, String(status)).toBe(satz);
			expect(e.message).not.toMatch(/Payload|\d{3} [A-Z]/);
			expect(statusSatz(status)).toBe(satz);
		}
		// JSON ohne Satz (oder leer): auch der Satz zum Status.
		const leer = await readError(
			new Response(JSON.stringify({ fehler: ' ', art: 'x' }), { status: 404 })
		);
		expect(leer.art).toBe('x');
		expect(leer.message).toBe('Das gibt es nicht (mehr).');
	});
	it('fehlerText: Satz des Servers, ohne Verbindung ein eigener Satz', async () => {
		const e = await getJson('/api/daten', antwort(409, { fehler: 'Schon erledigt.' })).catch(
			(x: unknown) => x
		);
		expect(fehlerText(e)).toBe('Schon erledigt.');
		const netz = await getJson('/api/daten', () =>
			Promise.reject(new TypeError('Failed to fetch'))
		).catch((x: unknown) => x);
		expect(fehlerText(netz)).toBe(SATZ_KEINE_VERBINDUNG);
	});
});
