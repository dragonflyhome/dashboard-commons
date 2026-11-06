import { beforeEach, describe, expect, it, vi } from 'vitest';
import { json } from './anstehend.beispiel.ts';
import type { ServerStand } from '$vertrag';

// Kopfleiste, Fassung 3, Auflage aus der Abnahme: Der Zähler der
// Aktualitätsprüfung bleibt beim Wechsel zwischen freiem und geschütztem
// Kanal genau. Kanäle und Server sind hier nachgebaut: Der Server zählt frei
// und voll getrennt, die Kanäle melden, was sie bringen.

type Hoerer = (...a: never[]) => void;
const hoerer = new Map<string, Set<Hoerer>>();
function an(name: string) {
	return (l: Hoerer): (() => void) => {
		const liste = hoerer.get(name) ?? new Set<Hoerer>();
		hoerer.set(name, liste.add(l));
		return () => liste.delete(l);
	};
}
function melden(name: string, ...a: unknown[]): void {
	for (const l of hoerer.get(name) ?? []) (l as (...x: unknown[]) => void)(...a);
}

const kanal = vi.hoisted((): { aktiv: 'frei' | 'geschuetzt' | null } => ({ aktiv: 'frei' }));
vi.mock('./freikanal.svelte.ts', () => ({
	freierKanal: {
		get aktiv() {
			return kanal.aktiv;
		},
		onInhalt: an('freiInhalt'),
		onFassung: an('fassung'),
		onBereit: an('bereit')
	}
}));
vi.mock('./live.svelte.ts', () => ({
	live: {
		onInhalt: an('liveInhalt'),
		onChange: an('change'),
		onVorgang: an('vorgang'),
		onBereich: an('bereich')
	}
}));

const { Aktualitaet } = await import('./aktualitaet.svelte.ts');

/** Der nachgebaute Server: zwei Zähler, `/api/stand/voll` nur angemeldet. */
const server = { frei: 0, voll: 0, inhalt: 10, angemeldet: false };
const stand = (ereignis: number): ServerStand => ({
	ereignis,
	inhalt: server.inhalt,
	fassung: 'build'
});
const gefragt: string[] = [];
/** Ist `halten` gesetzt, wartet die Antwort, bis der Test sie freigibt. */
let halten: Promise<void> | null = null;
const f = vi.fn(async (url: string) => {
	gefragt.push(url);
	const voll = url.endsWith('/voll');
	// Der Server liest seinen Stand beim Ankommen der Frage.
	const res =
		voll && !server.angemeldet
			? json({ fehler: 'x', art: 'anmeldung' }, 401)
			: json(stand(voll ? server.voll : server.frei));
	if (halten !== null) await halten;
	return res;
});
const fetchFn = f as unknown as typeof fetch;

/** Ein Ereignis, das nur der geschützte Kanal trägt (etwa `change`). */
function geschuetztesEreignis(ankommen = true): void {
	server.voll += 1;
	if (ankommen) melden('change');
}
/** Ein freies Ereignis (etwa `projekt`): beide Zähler; ankommen auf dem offenen Kanal. */
function freiesEreignis(kommtAn: 'frei' | 'geschuetzt' | null): void {
	server.frei += 1;
	server.voll += 1;
	server.inhalt += 1;
	const a = { projekte: ['schaufenster'], daten: [], sammel: false, version: server.inhalt };
	if (kommtAn === 'frei') melden('freiInhalt', a);
	if (kommtAn === 'geschuetzt') melden('liveInhalt', a);
}

async function bereit(k: 'frei' | 'geschuetzt'): Promise<void> {
	kanal.aktiv = k;
	server.angemeldet = k === 'geschuetzt';
	melden('bereit', k);
	await vi.waitFor(() => {
		expect(gefragt.at(-1)).toBe(k === 'geschuetzt' ? '/api/stand/voll' : '/api/stand');
	});
	// Die Antwort verarbeiten lassen.
	await new Promise((r) => setTimeout(r, 0));
}

const nachziehen = (): Promise<void> => Promise.resolve();

beforeEach(() => {
	hoerer.clear();
	Object.assign(server, { frei: 5, voll: 40, inhalt: 10, angemeldet: false });
	gefragt.length = 0;
	halten = null;
	kanal.aktiv = 'frei';
});

describe('Aktualitätsprüfung beim Kanalwechsel', () => {
	it('anmelden: danach zählt der geschützte Kanal gegen /api/stand/voll, kein falsches „Es fehlte etwas“', async () => {
		const a = new Aktualitaet();
		const aus = a.start(fetchFn);
		await vi.waitFor(() => {
			expect(a.zustand.basis?.stand.ereignis).toBe(5);
		});
		freiesEreignis('frei');
		expect(await a.pruefen(nachziehen)).toBe('aktuell');
		// Anmelden: Der geschützte steht; der Server zählt dort viel mehr (40 und mehr).
		await bereit('geschuetzt');
		expect(a.zustand.basis).toEqual({ stand: stand(41), voll: true });
		geschuetztesEreignis();
		freiesEreignis('geschuetzt');
		melden('fassung', { ordner: 'x', dauer_ms: 1 }, 'geschuetzt');
		server.voll += 1;
		gefragt.length = 0;
		expect(await a.pruefen(nachziehen)).toBe('aktuell');
		expect(gefragt).toEqual(['/api/stand/voll']);
		// Ein geschütztes Ereignis kam wirklich nicht an: das sagt die Prüfung.
		geschuetztesEreignis(false);
		expect(await a.pruefen(nachziehen)).toBe('nachgeholt');
		aus();
	});

	it('abmelden: danach zählt der freie Kanal gegen /api/stand, die geschützten Ereignisse zählen nicht mit', async () => {
		kanal.aktiv = 'geschuetzt';
		server.angemeldet = true;
		const a = new Aktualitaet();
		const aus = a.start(fetchFn);
		await vi.waitFor(() => {
			expect(a.zustand.basis).toEqual({ stand: stand(40), voll: true });
		});
		geschuetztesEreignis();
		// Abmelden: Der freie steht. Was der geschützte vorher brachte, zählt frei nicht.
		await bereit('frei');
		expect(a.zustand).toEqual({ basis: { stand: stand(5), voll: false }, empfangen: 0 });
		freiesEreignis('frei');
		melden('fassung', { ordner: 'y', dauer_ms: 1 }, 'frei');
		server.frei += 1;
		server.voll += 1;
		// Geschützte Schreibvorgänge ändern die freie Nummer nicht.
		geschuetztesEreignis(false);
		gefragt.length = 0;
		expect(await a.pruefen(nachziehen)).toBe('aktuell');
		expect(gefragt).toEqual(['/api/stand']);
		aus();
	});

	it('was ankommt, während die Frage nach dem Stand unterwegs ist, zählt (keine Lücke beim Wechsel)', async () => {
		const a = new Aktualitaet();
		const aus = a.start(fetchFn);
		await vi.waitFor(() => {
			expect(a.zustand.basis?.stand.ereignis).toBe(5);
		});
		// Anmelden; die Antwort auf /api/stand/voll lässt auf sich warten.
		let los: () => void = () => undefined;
		halten = new Promise((r) => (los = r));
		kanal.aktiv = 'geschuetzt';
		server.angemeldet = true;
		melden('bereit', 'geschuetzt');
		await vi.waitFor(() => {
			expect(gefragt.at(-1)).toBe('/api/stand/voll');
		});
		// Nach dem Lesen beim Server, vor der Antwort: ein Ereignis kommt an.
		geschuetztesEreignis();
		los();
		halten = null;
		await vi.waitFor(() => {
			expect(a.zustand.basis?.voll).toBe(true);
		});
		expect(a.zustand).toEqual({ basis: { stand: stand(40), voll: true }, empfangen: 1 });
		expect(await a.pruefen(nachziehen)).toBe('aktuell');
		aus();
	});

	it('Sitzung weg (401 auf /voll): kein Vergleich mit dem alten freien Bezug', async () => {
		const a = new Aktualitaet();
		const aus = a.start(fetchFn);
		await vi.waitFor(() => {
			expect(a.zustand.basis?.stand.ereignis).toBe(5);
		});
		await bereit('geschuetzt');
		// Während der geschützte galt, kamen freie Ereignisse nur dort an.
		freiesEreignis('geschuetzt');
		freiesEreignis('geschuetzt');
		// Widerruf: /voll gibt 401, die Seite fragt frei; der alte freie Bezug gilt nicht.
		server.angemeldet = false;
		expect(await a.pruefen(nachziehen)).toBe('abgeglichen');
		aus();
	});
});
