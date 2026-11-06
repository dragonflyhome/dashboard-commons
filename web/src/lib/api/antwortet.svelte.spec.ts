import { afterEach, describe, expect, it } from 'vitest';
import { apiStand } from './antwortet.svelte.ts';
import { getJson, postJson } from './client.ts';

// Zustand 1: „die API antwortet nicht“ heißt kein Netz,
// nicht eine Antwort mit Fehlercode.
afterEach(() => {
	apiStand.antwortet = true;
});

const netzfehler: typeof fetch = () => Promise.reject(new TypeError('Failed to fetch'));
const antwort =
	(status: number): typeof fetch =>
	() =>
		Promise.resolve(new Response('{}', { status }));

describe('apiStand: antwortet die API?', () => {
	it('Netzfehler: nein, bis wieder eine Antwort kommt', async () => {
		await expect(getJson('/api/probe', netzfehler)).rejects.toThrow(TypeError);
		expect(apiStand.antwortet).toBe(false);
		await expect(postJson('/api/probe', netzfehler)).rejects.toThrow(TypeError);
		expect(apiStand.antwortet).toBe(false);
		await getJson('/api/probe', antwort(200));
		expect(apiStand.antwortet).toBe(true);
	});

	it('eine Antwort mit 5xx ist eine Antwort (etwa 503 speicher-fehlt)', async () => {
		apiStand.antwortet = false;
		await expect(getJson('/api/probe', antwort(503))).rejects.toThrow();
		expect(apiStand.antwortet).toBe(true);
	});

	it('ein Abbruch (AbortError) ändert nichts', async () => {
		const abbruch: typeof fetch = () =>
			Promise.reject(new DOMException('abgebrochen', 'AbortError'));
		await expect(getJson('/api/probe', abbruch)).rejects.toThrow();
		expect(apiStand.antwortet).toBe(true);
	});
});
