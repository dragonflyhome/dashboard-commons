import { afterEach, describe, expect, it, vi } from 'vitest';
import { bauLage } from './bau.svelte.ts';
import { FREIER_KANAL, FreierKanal } from './freikanal.svelte.ts';
import { live } from './live.svelte.ts';
import { SUCHEN_MS } from './verbindung.ts';
import { zugang } from './zugang.ts';

// Kopfleiste, Fassung 2: der freie Kanal, die fünf Zustände und (Nachbesserung
// aus der Abnahme) genau EINE offene EventSource je Seite.

/** Kleiner Ersatz für EventSource (wie in live.spec.ts), mit Liste aller offenen. */
class FakeSource {
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSED = 2;
	static alle: FakeSource[] = [];
	readyState = FakeSource.CONNECTING;
	onopen: (() => void) | null = null;
	onerror: (() => void) | null = null;
	#handler = new Map<string, (ev: MessageEvent<string>) => void>();
	geschlossen = false;
	constructor(readonly url: string) {
		FakeSource.alle.push(this);
	}
	/** Die offenen (nicht geschlossenen) Quellen, als Adressen. */
	static offen(): string[] {
		return FakeSource.alle.filter((q) => !q.geschlossen).map((q) => q.url);
	}
	static letzte(url: string): FakeSource {
		const q = FakeSource.alle.filter((x) => x.url === url).at(-1);
		if (q === undefined) throw new Error(`keine Quelle für ${url}`);
		return q;
	}
	oeffnen(): void {
		this.readyState = FakeSource.OPEN;
		this.onopen?.();
	}
	fehlerNeuVerbinden(): void {
		this.readyState = FakeSource.CONNECTING;
		this.onerror?.();
	}
	fehlerAufgegeben(): void {
		this.readyState = FakeSource.CLOSED;
		this.onerror?.();
	}
	addEventListener(name: string, fn: (ev: MessageEvent<string>) => void): void {
		this.#handler.set(name, fn);
	}
	close(): void {
		this.geschlossen = true;
		this.readyState = FakeSource.CLOSED;
	}
	senden(name: string, daten: unknown): void {
		this.#handler.get(name)?.({ data: JSON.stringify(daten) } as MessageEvent<string>);
	}
}

const GESCHUETZT = '/api/events';
const frei = () => FakeSource.letzte(FREIER_KANAL);
const geschuetzt = () => FakeSource.letzte(GESCHUETZT);

/** Antwortet auf die Frage nach der Anmeldung mit `status` (und Art). */
function antwort(status: number, art = 'anmeldung'): typeof fetch {
	return vi.fn(() =>
		Promise.resolve(
			status === 200
				? new Response('{}', { status })
				: new Response(JSON.stringify({ fehler: 'x', art }), { status })
		)
	);
}

const ohneFetch = vi.fn(() => Promise.reject(new Error('kein Netz')));
let kanal: FreierKanal | null = null;

function neuerKanal(): FreierKanal {
	kanal = new FreierKanal();
	return kanal;
}

afterEach(() => {
	kanal?.stop();
	kanal = null;
	live.stop();
	vi.unstubAllGlobals();
	vi.useRealTimers();
	zugang.vergessen();
	FakeSource.alle = [];
});

describe('freier Kanal: fünf Zustände', () => {
	it('öffnet ohne Anmeldung nur /api/events/frei', () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('fehlt');
		const k = neuerKanal();
		k.start(ohneFetch);
		expect(FakeSource.offen()).toEqual([FREIER_KANAL]);
		expect(FREIER_KANAL).toBe('/api/events/frei');
		expect(k.aktiv).toBe('frei');
	});

	it('„live, nicht angemeldet“ und „nicht registriert“ aus dem Wort des Servers', () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('fehlt');
		const k = neuerKanal();
		k.start(ohneFetch);
		expect(k.zustand).toBeNull();
		frei().oeffnen();
		frei().senden('anmeldung', { stand: 'nicht_angemeldet' });
		expect(k.zustand).toBe('nicht_angemeldet');
		frei().senden('anmeldung', { stand: 'nicht_registriert' });
		expect(k.zustand).toBe('nicht_registriert');
		// Unbekanntes Wort: bleibt beim letzten.
		frei().senden('anmeldung', { stand: 'vielleicht' });
		expect(k.zustand).toBe('nicht_registriert');
	});

	it('„Verbindung nicht gefunden“: nie verbunden, nach Fehler oder Frist', () => {
		vi.useFakeTimers();
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('fehlt');
		const k = neuerKanal();
		k.start(ohneFetch);
		frei().fehlerNeuVerbinden();
		expect(k.zustand).toBe('nicht_gefunden');
		k.stop();
		const k2 = neuerKanal();
		k2.start(ohneFetch);
		expect(k2.zustand).toBeNull();
		vi.advanceTimersByTime(SUCHEN_MS);
		expect(k2.zustand).toBe('nicht_gefunden');
	});

	it('„Verbindung unterbrochen“: stand und ist weg; neu verbunden wieder live', () => {
		vi.useFakeTimers();
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('fehlt');
		const k = neuerKanal();
		k.start(ohneFetch);
		frei().oeffnen();
		frei().senden('anmeldung', { stand: 'nicht_angemeldet' });
		frei().fehlerNeuVerbinden();
		expect(k.zustand).toBe('unterbrochen');
		frei().oeffnen();
		expect(k.zustand).toBe('nicht_angemeldet');
		// Gibt der Browser auf, versucht es der Kanal selbst nach 2 s neu.
		const alt = frei();
		alt.fehlerAufgegeben();
		expect(k.zustand).toBe('unterbrochen');
		alt.close();
		expect(FakeSource.offen()).toEqual([]);
		vi.advanceTimersByTime(2000);
		expect(frei()).not.toBe(alt);
		frei().oeffnen();
		expect(k.zustand).toBe('nicht_angemeldet');
		expect(FakeSource.offen()).toEqual([FREIER_KANAL]);
	});

	it('fremdes Windows-Konto (401 Art passkey): fragt einmal nach dem Wort für das Konto', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		const f = vi.fn((url: string) =>
			Promise.resolve(
				url === '/api/vorgaenge/uebersicht'
					? new Response(JSON.stringify({ fehler: 'x', art: 'passkey' }), { status: 401 })
					: new Response(
							JSON.stringify({
								konto: 'PC\\fremd',
								hauptkonto: false,
								passkeys: [],
								anmeldung: 'nicht_registriert'
							}),
							{ status: 200 }
						)
			)
		) as unknown as typeof fetch;
		await zugang.pruefen(f);
		expect(zugang.art).toBe('passkey');
		const k = neuerKanal();
		k.start(f);
		frei().oeffnen();
		frei().senden('anmeldung', { stand: 'nicht_angemeldet' });
		await vi.waitFor(() => {
			expect(k.zustand).toBe('nicht_registriert');
		});
		expect(vi.mocked(f).mock.calls.map((c) => c[0])).toEqual([
			'/api/vorgaenge/uebersicht',
			'/api/passkey/antrag'
		]);
	});

	it('PC-Werte, Inhalt und Fassung kommen an', () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('fehlt');
		const k = neuerKanal();
		const inhalt: string[][] = [];
		const fassungen: string[] = [];
		k.onInhalt((a) => {
			inhalt.push(a.projekte);
		});
		k.onFassung((f) => {
			fassungen.push(f.ordner);
		});
		k.start(ohneFetch);
		frei().senden('pc', { at: 'x', cpu_percent: 5 });
		frei().senden('projekt', { slug: 'alpha', version: 3 });
		frei().senden('fassung', { ordner: '20261031-120000', dauer_ms: 9 });
		expect(k.pc?.cpu_percent).toBe(5);
		expect(inhalt).toEqual([['alpha']]);
		expect(fassungen).toEqual(['20261031-120000']);
	});
});

describe('genau ein Kanal je Seite', () => {
	it('angemeldet: nur der geschützte Kanal, Wort und Schloss von dort', () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('da');
		const k = neuerKanal();
		k.start(ohneFetch);
		expect(FakeSource.offen()).toEqual([GESCHUETZT]);
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		expect(k.zustand).toBe('angemeldet');
		expect(k.aktiv).toBe('geschuetzt');
		expect(FakeSource.offen()).toEqual([GESCHUETZT]);
	});

	it('Anmelden: der geschützte öffnet, der freie schließt erst bei dessen erstem Ereignis', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		const k = neuerKanal();
		// Beim Start war der Server nicht zu fragen (unbekannt); jetzt gibt er 200.
		k.start(antwort(200));
		frei().oeffnen();
		frei().senden('anmeldung', { stand: 'nicht_angemeldet' });
		await vi.waitFor(() => {
			expect(FakeSource.offen()).toEqual([FREIER_KANAL, GESCHUETZT]);
		});
		// Noch kein Ereignis vom geschützten: der freie bleibt, der Zustand ist live.
		expect(k.zustand).toBe('nicht_angemeldet');
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		expect(FakeSource.offen()).toEqual([GESCHUETZT]);
		expect(k.zustand).toBe('angemeldet');
	});

	it('Abmelden durch den Server (Widerruf): nie „nicht gefunden“, danach nur der freie', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('da');
		const k = neuerKanal();
		const zustaende: (string | null)[] = [];
		k.start(antwort(401));
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		zustaende.push(k.zustand);
		bauLage.fortschritt = { schritt: 'pruefung', fertig: 2, gesamt: 3 };
		// Der Server beendet die Sitzung: Schlusswort auf dem geschützten Kanal.
		geschuetzt().senden('anmeldung', { stand: 'nicht_angemeldet' });
		zustaende.push(k.zustand);
		expect(k.aktiv).toBe('frei');
		expect(bauLage.fortschritt, 'ohne Anmeldung kein Schrittzähler').toBeNull();
		// Übergabe: beide offen, bis der freie sein erstes Ereignis hat.
		expect(FakeSource.offen()).toEqual([GESCHUETZT, FREIER_KANAL]);
		frei().oeffnen();
		frei().senden('anmeldung', { stand: 'nicht_angemeldet' });
		zustaende.push(k.zustand);
		expect(FakeSource.offen()).toEqual([FREIER_KANAL]);
		expect(zustaende).toEqual(['angemeldet', 'nicht_angemeldet', 'nicht_angemeldet']);
		// Zurückgesetzt: die Seite fragt den Anmeldestand neu (401).
		await vi.waitFor(() => {
			expect(zugang.stand).toBe('fehlt');
		});
	});

	it('Knopf „Abmelden“: Zustand zurück, Schloss sofort zu, danach nur der freie', () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('da');
		const k = neuerKanal();
		k.start(antwort(401));
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		expect(k.zustand).toBe('angemeldet');
		bauLage.fortschritt = { schritt: 'vite_build', fertig: 0, gesamt: 1 };
		k.abgemeldet();
		expect(k.zustand).toBe('nicht_angemeldet');
		expect(bauLage.fortschritt, 'ohne Anmeldung kein Schrittzähler').toBeNull();
		expect(zugang.stand).not.toBe('da');
		frei().oeffnen();
		expect(FakeSource.offen()).toEqual([FREIER_KANAL]);
	});

	it('Ausweis abgelaufen (Neuaufbau 401): zum freien Kanal', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		vi.stubGlobal('fetch', antwort(401));
		zugang.setzen('da');
		const k = neuerKanal();
		k.start(antwort(401));
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		// Der Strom endet ohne Wort, der Neuaufbau bekommt 401: der Browser gibt auf.
		geschuetzt().fehlerAufgegeben();
		await vi.waitFor(() => {
			expect(k.aktiv).toBe('frei');
		});
		expect(FakeSource.offen()).toEqual([FREIER_KANAL]);
		frei().oeffnen();
		frei().senden('anmeldung', { stand: 'nicht_angemeldet' });
		expect(k.zustand).toBe('nicht_angemeldet');
	});

	it('Abmelden, aber Windows meldet noch an (200): zurück zum geschützten Kanal', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('da');
		const k = neuerKanal();
		k.start(antwort(200));
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		const alt = geschuetzt();
		// Passkey abgemeldet: Der Server beendet den Strom mit dem Schlusswort.
		alt.senden('anmeldung', { stand: 'nicht_angemeldet' });
		expect(k.aktiv).toBe('frei');
		// Die neue Frage sagt 200: ein neuer geschützter Kanal öffnet, der alte ist zu.
		await vi.waitFor(() => {
			expect(geschuetzt()).not.toBe(alt);
		});
		expect(alt.geschlossen).toBe(true);
		expect(FakeSource.offen()).toEqual([FREIER_KANAL, GESCHUETZT]);
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		expect(FakeSource.offen()).toEqual([GESCHUETZT]);
		expect(k.aktiv).toBe('geschuetzt');
		expect(k.zustand).toBe('angemeldet');
	});

	it('nie mehr als eine offene Quelle nach jedem Wechsel', () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('da');
		const k = neuerKanal();
		k.start(antwort(401));
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		expect(FakeSource.offen()).toHaveLength(1);
		k.abgemeldet();
		frei().oeffnen();
		frei().senden('anmeldung', { stand: 'nicht_angemeldet' });
		expect(FakeSource.offen()).toHaveLength(1);
		k.stop();
		expect(FakeSource.offen()).toHaveLength(0);
	});
});

describe('Meldungen für die Aktualitätsprüfung (Auflage aus der Abnahme der Kopfleiste 3)', () => {
	it('„bereit“ je Kanal erst, wenn sein Strom steht; beim Neuaufbau desselben Kanals nicht', async () => {
		vi.stubGlobal('EventSource', FakeSource);
		const k = neuerKanal();
		const bereit: string[] = [];
		k.onBereit((b) => bereit.push(b));
		// Start ohne Anmeldung: der freie, sobald er offen ist.
		k.start(antwort(200));
		expect(bereit).toEqual([]);
		frei().oeffnen();
		frei().senden('anmeldung', { stand: 'nicht_angemeldet' });
		expect(bereit).toEqual(['frei']);
		// Anmelden: erst mit dem ersten Ereignis des geschützten.
		await vi.waitFor(() => {
			expect(FakeSource.offen()).toEqual([FREIER_KANAL, GESCHUETZT]);
		});
		geschuetzt().oeffnen();
		expect(bereit).toEqual(['frei']);
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		expect(bereit).toEqual(['frei', 'geschuetzt']);
		// Ein Neuaufbau des geschützten (wieder „angemeldet“) meldet nichts Neues.
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		expect(bereit).toEqual(['frei', 'geschuetzt']);
		// Abmelden: der freie erst, wenn er offen ist (nicht schon beim Wechsel).
		k.abgemeldet();
		expect(k.aktiv).toBe('frei');
		expect(bereit).toEqual(['frei', 'geschuetzt']);
		frei().oeffnen();
		expect(bereit).toEqual(['frei', 'geschuetzt', 'frei']);
	});

	it('jede Fassung nennt den Kanal, aus dem sie kam', () => {
		vi.stubGlobal('EventSource', FakeSource);
		zugang.setzen('da');
		const k = neuerKanal();
		const fassungen: string[] = [];
		k.onFassung((f, woher) => fassungen.push(`${f.ordner}:${woher}`));
		k.start(antwort(401));
		geschuetzt().oeffnen();
		geschuetzt().senden('anmeldung', { stand: 'angemeldet' });
		geschuetzt().senden('fassung', { ordner: 'a', dauer_ms: 1 });
		k.abgemeldet();
		frei().oeffnen();
		frei().senden('fassung', { ordner: 'b', dauer_ms: 1 });
		expect(fassungen).toEqual(['a:geschuetzt', 'b:frei']);
	});
});
