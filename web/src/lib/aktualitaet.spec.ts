import { describe, expect, it, vi } from 'vitest';
import { Aktualitaet, fehlend, standHolen, type Basis } from './aktualitaet.svelte.ts';
import { json } from './anstehend.beispiel.ts';
import type { ServerStand, ServerStandFrei } from '$vertrag';

// Kopfleiste, Fassung 3: die Aktualitätsprüfung im Blatt der Brotkrumen.
const stand = (ereignis: number, inhalt = 10): ServerStand => ({
	ereignis,
	inhalt,
	fassung: 'build'
});
/** Schritt E2: der freie Stand ohne Inhaltsversion. */
const frei = (ereignis: number): ServerStandFrei => ({ ereignis, fassung: 'build' });
const basis = (ereignis: number, voll = false, inhalt = 10): Basis =>
	voll ? { stand: stand(ereignis, inhalt), voll } : { stand: frei(ereignis), voll };

describe('Aktualitätsprüfung: Vergleich mit dem Stand des Servers', () => {
	it('alles angekommen: nichts fehlt', () => {
		expect(fehlend(basis(5), 3, 0, basis(8))).toBe(0);
	});
	it('der Kanal hat Ereignisse verpasst: so viele fehlen', () => {
		expect(fehlend(basis(5), 1, 0, basis(8))).toBe(2);
	});
	it('nicht zu sagen: kein früherer Stand, anderer Weg, Server neu gestartet', () => {
		expect(fehlend(null, 0, 0, basis(1))).toBeNull();
		expect(fehlend(basis(5, false), 0, 0, basis(5, true))).toBeNull();
		expect(fehlend(basis(50), 0, 0, basis(3))).toBeNull();
	});
	it('angemeldet: eine weitere Inhaltsversion, die nie ankam, fehlt auch', () => {
		expect(fehlend(basis(5, true, 10), 0, 10, basis(5, true, 11))).toBe(1);
		expect(fehlend(basis(5, true, 10), 1, 11, basis(6, true, 11))).toBe(0);
		// Schritt E2: Frei gibt es keine Inhaltsversion, also auch kein Fehlen durch sie.
		expect(fehlend(basis(5), 0, 10, basis(5))).toBe(0);
	});
});

describe('Aktualitätsprüfung: den Stand holen', () => {
	it('angemeldet /api/stand/voll; 401 dort: frei', async () => {
		const f = vi.fn((url: string) =>
			Promise.resolve(
				url.endsWith('/voll') ? json({ fehler: 'x', art: 'anmeldung' }, 401) : json(frei(4))
			)
		);
		const b = await standHolen(true, f as unknown as typeof fetch);
		expect(f.mock.calls.map((c) => c[0])).toEqual(['/api/stand/voll', '/api/stand']);
		expect(b).toEqual({ stand: frei(4), voll: false });
	});
	it('ohne Netz: null, wirft nicht', async () => {
		const f = vi.fn(() => Promise.reject(new Error('weg')));
		expect(await standHolen(false, f as unknown as typeof fetch)).toBeNull();
	});
});

describe('Aktualitätsprüfung: Klick auf das Blatt', () => {
	it('zieht immer nach; „aktuell“, wenn nichts fehlte, „nachgeholt“, wenn etwas fehlte', async () => {
		let n = 3;
		const f = vi.fn(() => Promise.resolve(json(frei(n))));
		const a = new Aktualitaet();
		const aus = a.start(f);
		await vi.waitFor(() => {
			expect(a.zustand.basis?.stand.ereignis).toBe(3);
		});
		const nachziehen = vi.fn(() => Promise.resolve());
		expect(await a.pruefen(nachziehen)).toBe('aktuell');
		expect(nachziehen).toHaveBeenCalledTimes(1);
		// Zwei Ereignisse beim Server, keines kam an: nachgeholt.
		n = 5;
		expect(await a.pruefen(nachziehen)).toBe('nachgeholt');
		expect(nachziehen).toHaveBeenCalledTimes(2);
		expect(a.ergebnis).toBe('nachgeholt');
		expect(a.zustand).toEqual({ basis: { stand: frei(5), voll: false }, empfangen: 0 });
		aus();
		a.vergessen();
	});

	it('Server antwortet nicht: trotzdem nachziehen, Ergebnis „ohne-server“', async () => {
		const f = vi.fn(() => Promise.resolve(json({ fehler: 'weg' }, 503)));
		const a = new Aktualitaet();
		const aus = a.start(f);
		const nachziehen = vi.fn(() => Promise.resolve());
		expect(await a.pruefen(nachziehen)).toBe('ohne-server');
		expect(nachziehen).toHaveBeenCalledTimes(1);
		aus();
		a.vergessen();
	});
});
