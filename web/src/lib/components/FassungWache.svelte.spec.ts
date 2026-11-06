import { render } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { FassungEvent } from '$vertrag';

// Die Wache der Fassung (Etappe 4b; Vorgaben): Das
// Ereignis `fassung` mit neuerer Kennung macht die App „veraltet“; nur in
// Zustand 2 (verbunden, kein Bau) lädt die Seite nach 30 s von selbst neu,
// nicht während einer Eingabe, und nie ohne Verbindung.
const EIGENE = '20261104-120000';
const NEUER: FassungEvent = { ordner: '20261104-120500', dauer_ms: 4000 };
const { hoerer, navigation, kanal, stand } = vi.hoisted(() => ({
	hoerer: [] as ((f: FassungEvent) => void)[],
	navigation: [] as ((nav: unknown) => void)[],
	kanal: { quelle: (): string => 'offen', geschuetzt: (): boolean => true },
	stand: vi.fn()
}));
vi.mock('$app/env', () => ({ version: '20261104-120000' }));
vi.mock('$app/navigation', () => ({
	beforeNavigate: (f: (nav: unknown) => void) => navigation.push(f)
}));
vi.mock('#lib/aktualitaet.svelte.ts', () => ({ standHolen: stand }));
// Seit Kopfleiste 2 kommt `fassung` über den einen Kanal je Seite (frei oder geschützt).
vi.mock('#lib/freikanal.svelte.ts', () => ({
	freierKanal: {
		onFassung: (f: (e: FassungEvent) => void) => {
			hoerer.push(f);
			return () => hoerer.splice(hoerer.indexOf(f), 1);
		},
		get kanal() {
			return kanal.quelle();
		},
		get geschuetztOk() {
			return kanal.geschuetzt();
		}
	}
}));

const {
	FOKUS_MERKER,
	NACHFRAGE_MS,
	NACH_SPERRE_MS,
	PRUEF_MS,
	SELBST_NEU_MS,
	STAND_FRIST_MS,
	umgebung,
	zuruecksetzen
} = await import('#lib/fassung.ts');
const { apiStand } = await import('#lib/api/antwortet.svelte.ts');
const { fassung, umfeld } = await import('#lib/fassung.svelte.ts');
const { bauLage } = await import('#lib/bau.svelte.ts');
const { default: FassungWache } = await import('./FassungWache.svelte');

// Verbindung und Bau im Test von Hand (sonst freier Kanal und Bau-Lage).
// Mit `echt` der echte Weg ohne Anmeldung: Liste, `bauLage.liste`, `freiLaeuft`.
const lage = $state({ verbunden: true, baut: false, echt: false });
const echtBaut = umfeld.baut;
umfeld.verbunden = () => lage.verbunden;
umfeld.baut = () => (lage.echt ? echtBaut() : lage.baut);
const kanalStand = $state({ wert: 'offen', geschuetzt: true });
kanal.quelle = () => kanalStand.wert;
kanal.geschuetzt = () => kanalStand.geschuetzt;

const ursprung = { ...umgebung };
let neuLaden: ReturnType<typeof vi.fn<() => void>>;
let sichtbarkeit: DocumentVisibilityState = 'visible';

beforeEach(() => {
	vi.useFakeTimers();
	neuLaden = vi.fn<() => void>();
	umgebung.neuLaden = neuLaden;
	sichtbarkeit = 'visible';
	lage.verbunden = true;
	lage.baut = false;
	lage.echt = false;
	kanalStand.wert = 'offen';
	kanalStand.geschuetzt = true;
	stand.mockReset();
	Object.defineProperty(document, 'visibilityState', {
		configurable: true,
		get: () => sichtbarkeit
	});
});
afterEach(() => {
	vi.useRealTimers();
	Object.assign(umgebung, ursprung);
	sessionStorage.clear();
	zuruecksetzen();
	fassung.veraltet = false;
	apiStand.antwortet = true;
	document.body.innerHTML = '';
	bauLage.liste(null);
	vi.unstubAllGlobals();
});

/** Vor dem Neuladen fragt die Wache `/api/stand` (Promise): die Mikroaufgaben abwarten. */
async function geladen(): Promise<void> {
	for (let i = 0; i < 20; i += 1) await Promise.resolve();
}

/** Antwort von `/api/projects`: läuft ein Bau (ein Projekt mit `laeuft`) oder nicht. */
function liste(baut: boolean): Response {
	const projects = baut ? [{ slug: 'probe', bau: { zustand: 'laeuft', zeit: null } }] : [];
	// Das JSON erst in einer späteren Mikroaufgabe, wie beim echten Abruf.
	return {
		ok: true,
		status: 200,
		json: () => Promise.resolve().then(() => ({ projects }))
	} as unknown as Response;
}

/** Ersetzt `fetch` für `/api/projects`; `baut(n)` sagt, ob der n-te Abruf einen Bau nennt. */
function projekteAbruf(baut: (n: number) => boolean) {
	let n = 0;
	const abruf = vi.fn<(pfad: string, init?: RequestInit) => Promise<Response>>((pfad) => {
		if (!pfad.endsWith('/api/projects')) throw new Error(`unerwartet: ${pfad}`);
		n += 1;
		return Promise.resolve(liste(baut(n)));
	});
	vi.stubGlobal('fetch', abruf);
	return abruf;
}

/** Die Wache steht, und das Ereignis `fassung` kommt. */
function neueFassung(ereignis: FassungEvent = NEUER): void {
	render(FassungWache);
	hoerer.at(-1)?.(ereignis);
	flushSync();
}

describe('FassungWache', () => {
	it('gleiche oder ältere Kennung: nicht veraltet, kein Neuladen', () => {
		render(FassungWache);
		expect(hoerer).toHaveLength(1);
		hoerer[0]?.({ ordner: EIGENE, dauer_ms: 4000 });
		hoerer[0]?.({ ordner: '20261103-090000', dauer_ms: 4000 });
		flushSync();
		expect(fassung.veraltet).toBe(false);
		vi.advanceTimersByTime(SELBST_NEU_MS * 2);
		expect(neuLaden).not.toHaveBeenCalled();
	});

	it('neuere Fassung: veraltet, kein sofortiges Neuladen, nach 30 s von selbst', async () => {
		neueFassung();
		expect(fassung.lage).toBe('veraltet');
		expect(neuLaden).not.toHaveBeenCalled();
		vi.advanceTimersByTime(SELBST_NEU_MS - 1);
		expect(neuLaden).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		await geladen();
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('Abbruch der Verbindung während 2 stoppt das Neuladen; nie ohne Verbindung', async () => {
		neueFassung();
		vi.advanceTimersByTime(SELBST_NEU_MS - 1000);
		lage.verbunden = false;
		flushSync();
		expect(fassung.lage).toBe('keine_verbindung');
		vi.advanceTimersByTime(SELBST_NEU_MS * 10);
		expect(neuLaden).not.toHaveBeenCalled();
		// Wieder verbunden: Zustand 2 von vorn, wieder 30 s.
		lage.verbunden = true;
		flushSync();
		vi.advanceTimersByTime(SELBST_NEU_MS - 1);
		expect(neuLaden).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		await geladen();
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('Bau läuft (3): kein Neuladen; nach grünem Bau weiter mit 2', async () => {
		lage.baut = true;
		neueFassung();
		expect(fassung.lage).toBe('baut');
		vi.advanceTimersByTime(SELBST_NEU_MS * 5);
		expect(neuLaden).not.toHaveBeenCalled();
		lage.baut = false;
		flushSync();
		expect(fassung.lage).toBe('veraltet');
		vi.advanceTimersByTime(SELBST_NEU_MS);
		await geladen();
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('Sperrgrund: kein Neuladen, „aufgehalten“, danach nach kurzer Frist', async () => {
		document.body.innerHTML =
			'<div role="group"><details><textarea id="notiz"></textarea></details></div>';
		const feld = document.querySelector('textarea');
		if (feld === null) throw new Error('kein Feld');
		feld.value = 'Halb geschrieben';
		neueFassung();
		expect(fassung.aufgehalten).toBe(false);
		vi.advanceTimersByTime(SELBST_NEU_MS * 10);
		expect(neuLaden).not.toHaveBeenCalled();
		expect(fassung.aufgehalten).toBe(true);
		feld.value = '';
		vi.advanceTimersByTime(PRUEF_MS);
		expect(fassung.aufgehalten).toBe(false);
		vi.advanceTimersByTime(NACH_SPERRE_MS - 1);
		expect(neuLaden).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		await geladen();
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('kein Neuladen während der Eingabe (Fokus im Feld), einer offenen Antwort und eines Dialogs', async () => {
		document.body.innerHTML =
			'<input id="suche" type="search"><div role="group" aria-busy="false"></div><dialog id="d"></dialog>';
		const suche = document.querySelector<HTMLInputElement>('#suche');
		const gruppe = document.querySelector('[role="group"]');
		const dialog = document.querySelector('dialog');
		neueFassung();
		suche?.focus();
		vi.advanceTimersByTime(SELBST_NEU_MS * 3);
		expect(neuLaden).not.toHaveBeenCalled();
		suche?.blur();
		gruppe?.setAttribute('aria-busy', 'true');
		vi.advanceTimersByTime(SELBST_NEU_MS * 3);
		expect(neuLaden).not.toHaveBeenCalled();
		gruppe?.setAttribute('aria-busy', 'false');
		dialog?.setAttribute('open', '');
		vi.advanceTimersByTime(SELBST_NEU_MS * 3);
		expect(neuLaden).not.toHaveBeenCalled();
		dialog?.removeAttribute('open');
		vi.advanceTimersByTime(PRUEF_MS + SELBST_NEU_MS);
		await geladen();
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('ein gesicherter Entwurf ist kein Sperrgrund', async () => {
		document.body.innerHTML = '<form><textarea data-entwurf-gesichert></textarea></form>';
		const feld = document.querySelector('textarea');
		if (feld === null) throw new Error('kein Feld');
		feld.value = 'Steht schon in sessionStorage';
		neueFassung();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		await geladen();
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('Tab im Hintergrund: lädt erst, wenn er wieder sichtbar ist', async () => {
		sichtbarkeit = 'hidden';
		neueFassung();
		vi.advanceTimersByTime(SELBST_NEU_MS * 5);
		expect(neuLaden).not.toHaveBeenCalled();
		sichtbarkeit = 'visible';
		document.dispatchEvent(new Event('visibilitychange'));
		await geladen();
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('nach einer Unterbrechung holt /api/stand eine verpasste Fassung nach', async () => {
		stand.mockResolvedValue({
			stand: { ereignis: 1, inhalt: 1, fassung: NEUER.ordner },
			voll: false
		});
		kanalStand.wert = 'weg';
		render(FassungWache);
		flushSync();
		expect(stand).not.toHaveBeenCalled();
		kanalStand.wert = 'offen';
		flushSync();
		expect(stand).toHaveBeenCalledTimes(1);
		await vi.waitFor(() => {
			expect(fassung.veraltet).toBe(true);
		});
	});

	it.each(['nie', 'sucht'] as const)(
		'Kanal steht zum ersten Mal (%s -> offen): /api/stand holt ein Ereignis davor nach',
		async (anfang) => {
			stand.mockResolvedValue({
				stand: { ereignis: 1, inhalt: 1, fassung: NEUER.ordner },
				voll: false
			});
			kanalStand.wert = anfang;
			render(FassungWache);
			flushSync();
			kanalStand.wert = 'offen';
			flushSync();
			expect(stand).toHaveBeenCalledTimes(1);
			await vi.waitFor(() => {
				expect(fassung.veraltet).toBe(true);
			});
		}
	);

	it('API antwortet nicht bei stehendem Kanal: fragt alle paar Sekunden, bis wieder eine Antwort kommt', () => {
		stand.mockResolvedValue(null);
		render(FassungWache);
		flushSync();
		vi.advanceTimersByTime(NACHFRAGE_MS * 3);
		expect(stand).not.toHaveBeenCalled();
		apiStand.antwortet = false;
		flushSync();
		vi.advanceTimersByTime(NACHFRAGE_MS * 2);
		expect(stand).toHaveBeenCalledTimes(2);
		apiStand.antwortet = true;
		flushSync();
		vi.advanceTimersByTime(NACHFRAGE_MS * 3);
		expect(stand).toHaveBeenCalledTimes(2);
	});

	it('fällig, aber der Dienst antwortet nicht (Zustand 1): kein Neuladen; antwortet er wieder, von vorn', async () => {
		lage.verbunden = true;
		stand.mockImplementation(() => {
			lage.verbunden = false;
			return Promise.resolve(null);
		});
		neueFassung();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		await geladen();
		flushSync();
		expect(stand).toHaveBeenCalled();
		expect(fassung.lage).toBe('keine_verbindung');
		expect(neuLaden).not.toHaveBeenCalled();
		stand.mockResolvedValue(null);
		lage.verbunden = true;
		flushSync();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		await geladen();
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('veraltet: der nächste Klick auf einen Link navigiert voll', () => {
		const gehe = vi.fn<(href: string) => void>();
		umgebung.gehe = gehe;
		neueFassung();
		const cancel = vi.fn();
		navigation.at(-1)?.({
			from: { route: { id: '/pc' } },
			to: { url: new URL('http://localhost/anstehend'), route: { id: '/anstehend' } },
			willUnload: false,
			type: 'link',
			cancel
		});
		expect(cancel).toHaveBeenCalled();
		expect(gehe).toHaveBeenCalledWith('http://localhost/anstehend');
	});

	it('während der Frage nach /api/stand geht etwas auf (Text in einer Antwort): kein Neuladen, aufgehalten', async () => {
		document.body.innerHTML = '<textarea></textarea>';
		const feld = document.querySelector('textarea');
		if (feld === null) throw new Error('kein Feld');
		stand.mockImplementation(() => {
			feld.value = 'Während der Frage getippt';
			return Promise.resolve(null);
		});
		neueFassung();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		await geladen();
		expect(stand).toHaveBeenCalledTimes(1);
		expect(neuLaden).not.toHaveBeenCalled();
		expect(fassung.aufgehalten).toBe(true);
		vi.advanceTimersByTime(SELBST_NEU_MS * 3);
		expect(neuLaden).not.toHaveBeenCalled();
	});

	it('/api/stand mit Frist: der Abruf trägt ein Signal, das nach STAND_FRIST_MS abbricht', async () => {
		stand.mockResolvedValue(null);
		const holen = vi.fn((_: unknown, init?: RequestInit) =>
			Promise.resolve(new Response(String(init?.signal instanceof AbortSignal)))
		);
		vi.stubGlobal('fetch', holen);
		const zeit = vi.spyOn(AbortSignal, 'timeout');
		try {
			neueFassung();
			vi.advanceTimersByTime(SELBST_NEU_MS);
			await geladen();
			const abruf = stand.mock.calls[0]?.[1] as typeof fetch | undefined;
			expect(stand.mock.calls[0]?.[0]).toBe(false);
			await abruf?.('/api/stand', { cache: 'no-store' });
			expect(holen).toHaveBeenCalledTimes(1);
			expect(holen.mock.calls[0]?.[1]?.signal).toBeInstanceOf(AbortSignal);
			expect(holen.mock.calls[0]?.[1]?.cache).toBe('no-store');
			expect(zeit).toHaveBeenCalledWith(STAND_FRIST_MS);
		} finally {
			zeit.mockRestore();
			vi.unstubAllGlobals();
		}
	});

	it('ohne Anmeldung: holt die Projektliste beim Ereignis fassung, alle paar Sekunden während 3 und vor dem Neuladen', async () => {
		kanalStand.geschuetzt = false;
		lage.echt = true;
		let baut = false;
		const abruf = projekteAbruf(() => baut);
		neueFassung();
		expect(abruf).toHaveBeenCalledTimes(1);
		expect(abruf.mock.calls[0]?.[1]?.signal).toBeInstanceOf(AbortSignal);
		// Ein weiteres Ereignis fassung; die Liste sagt: ein Bau läuft (Zustand 3).
		baut = true;
		hoerer.at(-1)?.(NEUER);
		await geladen();
		flushSync();
		expect(abruf).toHaveBeenCalledTimes(2);
		expect(fassung.lage).toBe('baut');
		// Bis sie es nicht mehr sagt, alle NACHFRAGE_MS (so endet 3 auch nach rotem Bau).
		vi.advanceTimersByTime(NACHFRAGE_MS * 2);
		await geladen();
		expect(abruf).toHaveBeenCalledTimes(4);
		baut = false;
		vi.advanceTimersByTime(NACHFRAGE_MS);
		await geladen();
		flushSync();
		expect(fassung.lage).toBe('veraltet');
		expect(abruf).toHaveBeenCalledTimes(5);
		vi.advanceTimersByTime(NACHFRAGE_MS * 2);
		expect(abruf).toHaveBeenCalledTimes(5);
		// Zustand 2: direkt vor dem Neuladen noch einmal die Liste.
		vi.advanceTimersByTime(SELBST_NEU_MS - NACHFRAGE_MS * 2);
		await geladen();
		expect(abruf).toHaveBeenCalledTimes(6);
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('ohne Anmeldung: nennt die Liste vor dem Neuladen einen neuen Bau, gilt 3 und nichts lädt (echter Weg)', async () => {
		kanalStand.geschuetzt = false;
		lage.echt = true;
		const abruf = projekteAbruf((n) => n > 1);
		neueFassung();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		await geladen();
		flushSync();
		expect(abruf).toHaveBeenCalledTimes(2);
		expect(bauLage.freiLaeuft).toBe(true);
		expect(fassung.lage).toBe('baut');
		expect(neuLaden).not.toHaveBeenCalled();
	});

	it('ohne Anmeldung: eine ältere Liste, die später ankommt, überschreibt die neuere nicht', async () => {
		kanalStand.geschuetzt = false;
		const antworten: ((r: Response) => void)[] = [];
		vi.stubGlobal(
			'fetch',
			vi.fn(
				() =>
					new Promise<Response>((fertig) => {
						antworten.push(fertig);
					})
			)
		);
		neueFassung();
		hoerer.at(-1)?.(NEUER);
		expect(antworten).toHaveLength(2);
		antworten[1]?.(liste(false));
		await geladen();
		antworten[0]?.(liste(true));
		await geladen();
		expect(bauLage.freiLaeuft).toBe(false);
	});

	it('ohne Anmeldung: Liste ohne Antwort ändert den Bau-Stand nicht und wirft nicht', async () => {
		kanalStand.geschuetzt = false;
		lage.echt = true;
		bauLage.liste([{ slug: 'probe', bau: { zustand: 'laeuft', zeit: null } }] as never);
		vi.stubGlobal(
			'fetch',
			vi.fn(() => Promise.reject(new TypeError('Failed to fetch')))
		);
		neueFassung();
		await geladen();
		expect(bauLage.freiLaeuft).toBe(true);
		expect(apiStand.antwortet).toBe(false);
	});

	it('angemeldet: keine Liste (das Ereignis bau kommt live)', async () => {
		const abruf = projekteAbruf(() => false);
		lage.baut = true;
		neueFassung();
		vi.advanceTimersByTime(NACHFRAGE_MS * 3);
		lage.baut = false;
		flushSync();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		await geladen();
		expect(abruf).not.toHaveBeenCalled();
	});

	it('nach einem Neuladen durch die Fassung: Fokus einmal zurück aufs Feld, sonst auf main', () => {
		document.body.innerHTML = '<main tabindex="-1"></main><input id="notiz">';
		sessionStorage.setItem(FOKUS_MERKER, 'notiz');
		render(FassungWache);
		expect(document.activeElement?.id).toBe('notiz');
		expect(sessionStorage.getItem(FOKUS_MERKER)).toBeNull();
	});

	it('zeigt selbst nichts (der Zustand steht am Live-Punkt)', () => {
		neueFassung();
		expect(document.body.textContent.trim()).toBe('');
		expect(document.querySelector('button, [role="status"]')).toBeNull();
	});
});
