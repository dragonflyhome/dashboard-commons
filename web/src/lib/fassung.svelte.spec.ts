import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	FOKUS_MERKER,
	LIVE_PUNKT,
	MERKER,
	NACH_SPERRE_MS,
	PRUEF_MS,
	SELBST_NEU_MS,
	SPERRE_MS,
	WIEDER_MS,
	darfNeuLaden,
	eingabeOffen,
	einmalNeuLaden,
	fokusMerken,
	fokusZurueck,
	istChunkFehler,
	kennungZeit,
	lageVon,
	merkerLesen,
	neuere,
	neuLadenAufschieben,
	selbstNeuLaden,
	umgebung,
	ungesichertOffen,
	zuruecksetzen
} from './fassung.ts';
import type { ProjectSummary } from '$vertrag';
import { bauLage } from './bau.svelte.ts';
import { Fassung, umfeld as echtesUmfeld, type Navigation, type Umfeld } from './fassung.svelte.ts';
import { freierKanal } from './freikanal.svelte.ts';
import { live } from './live.svelte.ts';

// Neue Fassung nach einem Bau (Etappe 4b, Nachtrag): die Regeln und
// der Zustand. Neuladen und Navigation sind ersetzt; nichts lädt wirklich.
const ursprung = { ...umgebung };
let uhr = 1_000_000;
let neuLaden: ReturnType<typeof vi.fn<() => void>>;
let gehe: ReturnType<typeof vi.fn<(href: string) => void>>;

beforeEach(() => {
	uhr = 1_000_000;
	neuLaden = vi.fn<() => void>();
	gehe = vi.fn<(href: string) => void>();
	umgebung.jetzt = () => uhr;
	umgebung.neuLaden = neuLaden;
	umgebung.gehe = gehe;
	sessionStorage.clear();
	zuruecksetzen();
	document.body.innerHTML = '';
});
afterEach(() => {
	Object.assign(umgebung, ursprung);
	document.body.innerHTML = '';
});

/** Als wäre die Seite neu geladen: dieselbe sessionStorage, neue Seite. */
function seiteNeu(): void {
	zuruecksetzen();
}

describe('darfNeuLaden: höchstens einmal, keine Schleife', () => {
	it('ohne Merker ja', () => {
		expect(darfNeuLaden(null, 0, 'a')).toBe(true);
	});
	it('innerhalb der Sperre nie, auch nicht aus einer anderen Fassung', () => {
		expect(darfNeuLaden({ zeit: 0, von: 'a' }, SPERRE_MS - 1, 'b')).toBe(false);
		expect(darfNeuLaden({ zeit: 0, von: 'a' }, SPERRE_MS - 1, 'a')).toBe(false);
	});
	it('aus derselben Fassung erst nach WIEDER_MS', () => {
		expect(darfNeuLaden({ zeit: 0, von: 'a' }, SPERRE_MS, 'a')).toBe(false);
		expect(darfNeuLaden({ zeit: 0, von: 'a' }, WIEDER_MS, 'a')).toBe(true);
	});
	it('aus einer neueren Fassung nach der Sperre wieder', () => {
		expect(darfNeuLaden({ zeit: 0, von: 'a' }, SPERRE_MS, 'b')).toBe(true);
	});
	it('ein Merker aus der Zukunft (Uhr verstellt) sperrt nicht für immer', () => {
		expect(darfNeuLaden({ zeit: 10 * WIEDER_MS, von: 'a' }, 0, 'b')).toBe(true);
	});
	it('ein unlesbarer Merker gilt als keiner', () => {
		sessionStorage.setItem(MERKER, '{kaputt');
		expect(merkerLesen(sessionStorage)).toBeNull();
		sessionStorage.setItem(MERKER, JSON.stringify({ zeit: 'x' }));
		expect(merkerLesen(sessionStorage)).toBeNull();
		expect(merkerLesen(null)).toBeNull();
	});
});

describe('istChunkFehler', () => {
	it('erkennt fehlende Teile des Baus in Edge, Firefox, Safari und Vite', () => {
		for (const satz of [
			'Failed to fetch dynamically imported module: http://localhost:8787/_app/immutable/nodes/4.abc.js',
			'error loading dynamically imported module: http://localhost/_app/x.js',
			'Importing a module script failed.',
			'Unable to preload CSS for /_app/immutable/assets/4.css'
		]) {
			expect(istChunkFehler(new TypeError(satz)), satz).toBe(true);
		}
		expect(istChunkFehler({ message: 'Importing a module script failed.' })).toBe(true);
	});
	it('ein gewöhnlicher Netzfehler oder ein Fehler der Seite ist keiner', () => {
		expect(istChunkFehler(new TypeError('Failed to fetch'))).toBe(false);
		expect(istChunkFehler(new Error('Teil des Baus fehlt'))).toBe(false);
		expect(istChunkFehler(null)).toBe(false);
	});
});

describe('eingabeOffen', () => {
	function feld(html: string, fokus = 'input, textarea, select, [contenteditable]'): void {
		document.body.innerHTML = html;
		document.querySelector<HTMLElement>(fokus)?.focus();
	}
	it('nichts offen: nein', () => {
		feld('<a href="/">Start</a>');
		expect(eingabeOffen(document)).toBe(false);
	});
	it('ein Schreibfeld mit Fokus: ja (input, textarea, select, contenteditable)', () => {
		for (const html of [
			'<input type="text">',
			'<input>',
			'<input type="search">',
			'<textarea></textarea>',
			'<select><option>a</option></select>',
			'<div contenteditable="true" tabindex="0">x</div>'
		]) {
			feld(html);
			expect(eingabeOffen(document), html).toBe(true);
		}
	});
	it('ein Häkchen oder Knopf mit Fokus ist keine offene Eingabe', () => {
		feld('<input type="checkbox">');
		expect(eingabeOffen(document)).toBe(false);
		feld('<button>ok</button>', 'button');
		expect(eingabeOffen(document)).toBe(false);
	});
	it('ein offener Dialog oder ein Popover mit Formular: ja, auch ohne Fokus', () => {
		feld('<dialog open><form><button>ok</button></form></dialog>', 'nichts');
		expect(eingabeOffen(document)).toBe(true);
		feld('<div role="dialog"><input type="text"></div>', 'nichts');
		expect(eingabeOffen(document)).toBe(true);
		feld('<dialog open><p>Nur Text</p></dialog>', 'nichts');
		expect(eingabeOffen(document)).toBe(false);
		feld('<dialog><form></form></dialog>', 'nichts');
		expect(eingabeOffen(document)).toBe(false);
	});
});

describe('neuLadenAufschieben: nichts wegladen, was verloren ginge', () => {
	it('nichts offen, nur Suchfelder oder leere Felder: nein', () => {
		document.body.innerHTML =
			'<input type="search" value="abc"><form><input type="text"><textarea></textarea></form>';
		expect(neuLadenAufschieben(document)).toBe(false);
	});
	it('Fokus in einem Schreibfeld: ja', () => {
		document.body.innerHTML = '<input type="search">';
		document.querySelector('input')?.focus();
		expect(neuLadenAufschieben(document)).toBe(true);
	});
	it('ein offener Dialog, auch ohne Felder: ja', () => {
		document.body.innerHTML = '<dialog open><p>Nur Text</p></dialog>';
		expect(neuLadenAufschieben(document)).toBe(true);
		document.body.innerHTML = '<div role="dialog"><p>Schublade</p></div>';
		expect(neuLadenAufschieben(document)).toBe(true);
		document.body.innerHTML = '<dialog><p>zu</p></dialog>';
		expect(neuLadenAufschieben(document)).toBe(false);
	});
	it('eine ungesicherte Antwort ohne Fokus: Text im Feld oder gerade gesendet', () => {
		document.body.innerHTML = '<details><textarea></textarea></details>';
		const t = document.querySelector('textarea');
		if (t === null) throw new Error('kein Feld');
		t.value = 'Meine Anmerkung';
		expect(ungesichertOffen(document)).toBe(true);
		expect(neuLadenAufschieben(document)).toBe(true);
		t.value = '   ';
		expect(ungesichertOffen(document)).toBe(false);
		document.body.innerHTML = '<div role="group" aria-busy="true"><button>Ja</button></div>';
		expect(ungesichertOffen(document)).toBe(true);
		document.body.innerHTML = '<div role="group" aria-busy="false"><button>Ja</button></div>';
		expect(ungesichertOffen(document)).toBe(false);
	});
	it('Rückfrage „Freigabe bestätigen“ (data-sperrt-neuladen) ist ein Sperrgrund, auch ohne Felder', () => {
		document.body.innerHTML =
			'<div class="frage" role="group" aria-label="Freigabe bestätigen" data-sperrt-neuladen><button>Ja</button></div>';
		expect(ungesichertOffen(document)).toBe(true);
		expect(neuLadenAufschieben(document)).toBe(true);
		document.body.innerHTML =
			'<div class="frage" role="group" aria-label="Freigabe bestätigen"><button>Ja</button></div>';
		expect(neuLadenAufschieben(document)).toBe(false);
	});
	it('laufende Passkey-Abfrage (aria-busy am Knopf, /anmelden und „Mit Passkey bestätigen“): Sperrgrund', () => {
		document.body.innerHTML =
			'<p><button type="button" aria-busy="true">Bitte am Gerät bestätigen …</button></p>';
		expect(neuLadenAufschieben(document)).toBe(true);
		document.body.innerHTML =
			'<p><button type="button" aria-busy="false">Mit Passkey anmelden</button></p>';
		expect(neuLadenAufschieben(document)).toBe(false);
	});
	it('ein ausgefülltes Feld in einem Formular zählt, ein schreibgeschütztes nicht', () => {
		document.body.innerHTML =
			'<form><input id="a" type="text"><input id="b" type="text" readonly value="x"></form>';
		expect(ungesichertOffen(document)).toBe(false);
		const a = document.querySelector<HTMLInputElement>('#a');
		if (a === null) throw new Error('kein Feld');
		a.value = 'Handy';
		expect(ungesichertOffen(document)).toBe(true);
	});
	it('ein gesicherter Entwurf (data-entwurf-gesichert, „Etwas anderes …“) ist kein Sperrgrund', () => {
		document.body.innerHTML =
			'<form><textarea data-entwurf-gesichert></textarea><input data-entwurf-gesichert></form>';
		const t = document.querySelector('textarea');
		const i = document.querySelector('input');
		if (t === null || i === null) throw new Error('kein Feld');
		t.value = 'Schon in sessionStorage';
		i.value = 'auch';
		expect(ungesichertOffen(document)).toBe(false);
		expect(neuLadenAufschieben(document)).toBe(false);
		// Mit Fokus im Feld wartet es trotzdem (der Benutzer schreibt gerade).
		t.focus();
		expect(neuLadenAufschieben(document)).toBe(true);
	});
});

describe('selbstNeuLaden: nach 30 s, nicht während einer Eingabe', () => {
	let offen = false;
	let verborgen = false;
	let sichtbar: (() => void) | null = null;
	let laden: ReturnType<typeof vi.fn<() => boolean | Promise<boolean>>>;
	let stop: () => void = () => undefined;

	let aufgehalten: boolean[] = [];

	function starten(): void {
		stop = selbstNeuLaden({
			aufschieben: () => offen,
			verborgen: () => verborgen,
			beiSichtbar: (f) => {
				sichtbar = f;
				return () => {
					sichtbar = null;
				};
			},
			laden,
			aufgehalten: (ja) => aufgehalten.push(ja)
		});
	}

	it('Sperrgrund: meldet einmal aufgehalten und einmal vorbei (für den Text)', () => {
		starten();
		offen = true;
		vi.advanceTimersByTime(SELBST_NEU_MS + 5 * PRUEF_MS);
		expect(aufgehalten).toEqual([true]);
		offen = false;
		vi.advanceTimersByTime(PRUEF_MS);
		expect(aufgehalten).toEqual([true, false]);
		expect(laden).not.toHaveBeenCalled();
	});

	beforeEach(() => {
		vi.useFakeTimers();
		offen = false;
		verborgen = false;
		sichtbar = null;
		aufgehalten = [];
		laden = vi.fn<() => boolean | Promise<boolean>>(() => true);
	});
	afterEach(() => {
		stop();
		vi.useRealTimers();
	});

	it('nichts offen: lädt nach genau 30 s, nicht früher', () => {
		starten();
		vi.advanceTimersByTime(SELBST_NEU_MS - 1);
		expect(laden).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(laden).toHaveBeenCalledTimes(1);
	});

	it('während einer Eingabe nie; ist sie vorbei, nach kurzer Frist (nicht 30 s)', () => {
		starten();
		offen = true;
		vi.advanceTimersByTime(SELBST_NEU_MS + 10 * 60_000);
		expect(laden).not.toHaveBeenCalled();
		offen = false;
		vi.advanceTimersByTime(PRUEF_MS);
		// Vorbei („sobald du fertig bist“): kurz warten, nicht sofort.
		vi.advanceTimersByTime(NACH_SPERRE_MS - 1);
		expect(laden).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(laden).toHaveBeenCalledTimes(1);
		expect(NACH_SPERRE_MS).toBeLessThan(SELBST_NEU_MS);
	});

	it('in der kurzen Frist wieder etwas offen: wieder aufgehalten, kein Neuladen', () => {
		starten();
		offen = true;
		vi.advanceTimersByTime(SELBST_NEU_MS + PRUEF_MS);
		offen = false;
		vi.advanceTimersByTime(PRUEF_MS);
		offen = true;
		vi.advanceTimersByTime(NACH_SPERRE_MS + 5 * PRUEF_MS);
		expect(laden).not.toHaveBeenCalled();
		expect(aufgehalten).toEqual([true, false, true]);
	});

	it('im Hintergrund nicht; sobald der Tab sichtbar ist, gleich', () => {
		verborgen = true;
		starten();
		vi.advanceTimersByTime(SELBST_NEU_MS * 10);
		expect(laden).not.toHaveBeenCalled();
		expect(sichtbar).not.toBeNull();
		verborgen = false;
		sichtbar?.();
		expect(laden).toHaveBeenCalledTimes(1);
	});

	it('wieder sichtbar, aber eine Eingabe offen: erst danach und nach kurzer Frist', () => {
		verborgen = true;
		starten();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		verborgen = false;
		offen = true;
		sichtbar?.();
		vi.advanceTimersByTime(SELBST_NEU_MS * 3);
		expect(laden).not.toHaveBeenCalled();
		offen = false;
		vi.advanceTimersByTime(PRUEF_MS + NACH_SPERRE_MS - 1);
		expect(laden).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		expect(laden).toHaveBeenCalledTimes(1);
	});

	it('darf gerade nicht (Schutz vor Schleifen): später noch einmal', () => {
		laden.mockReturnValueOnce(false);
		starten();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		expect(laden).toHaveBeenCalledTimes(1);
		// Nichts hält auf: „aufgehalten“ (etwa nach einem Klick, jetztLaden) endet hier.
		expect(aufgehalten).toEqual([false]);
		vi.advanceTimersByTime(SELBST_NEU_MS);
		expect(laden).toHaveBeenCalledTimes(2);
	});

	it('fragt erst (Promise) und darf dann nicht: später noch einmal', async () => {
		laden.mockReturnValueOnce(Promise.resolve(false));
		starten();
		await vi.advanceTimersByTimeAsync(SELBST_NEU_MS);
		expect(laden).toHaveBeenCalledTimes(1);
		await vi.advanceTimersByTimeAsync(SELBST_NEU_MS);
		expect(laden).toHaveBeenCalledTimes(2);
	});

	it('während der Frage (Promise) geht etwas auf: aufgehalten statt in 30 s noch einmal', async () => {
		laden.mockImplementationOnce(() => {
			offen = true;
			return Promise.resolve(false);
		});
		starten();
		await vi.advanceTimersByTimeAsync(SELBST_NEU_MS);
		expect(aufgehalten).toEqual([true]);
		offen = false;
		await vi.advanceTimersByTimeAsync(PRUEF_MS + NACH_SPERRE_MS);
		expect(aufgehalten).toEqual([true, false]);
		expect(laden).toHaveBeenCalledTimes(2);
	});

	it('abgemeldet: keine Uhr, kein Hörer mehr', () => {
		verborgen = true;
		starten();
		vi.advanceTimersByTime(SELBST_NEU_MS);
		stop();
		expect(sichtbar).toBeNull();
		verborgen = false;
		vi.advanceTimersByTime(SELBST_NEU_MS * 2);
		expect(laden).not.toHaveBeenCalled();
	});
});

describe('lageVon: drei Zustände, Vorrang 1 vor 3 vor 2', () => {
	it('keine Verbindung geht vor Bau und veraltet', () => {
		expect(lageVon(false, true, true)).toBe('keine_verbindung');
		expect(lageVon(false, false, false)).toBe('keine_verbindung');
	});
	it('Bau läuft geht vor veraltet', () => {
		expect(lageVon(true, true, true)).toBe('baut');
		expect(lageVon(true, true, false)).toBe('baut');
	});
	it('veraltet nur verbunden und ohne Bau', () => {
		expect(lageVon(true, false, true)).toBe('veraltet');
	});
	it('nur TOML geändert (kein Bau, keine neue Fassung): kein Zustand', () => {
		expect(lageVon(true, false, false)).toBeNull();
	});
	it('solange der erste Versuch läuft: keiner', () => {
		expect(lageVon(null, true, true)).toBeNull();
	});
});

describe('neuere: nur eine neuere Bau-Kennung macht die App veraltet', () => {
	const eigene = '20261104-120000';
	it('gleich: nein', () => {
		expect(neuere(eigene, eigene)).toBe(false);
	});
	it('älter: nein', () => {
		expect(neuere('20261104-115959', eigene)).toBe(false);
		expect(neuere('20261031-235959-7', eigene)).toBe(false);
	});
	it('neuer: ja, auch in derselben Sekunde mit Nummer', () => {
		expect(neuere('20261104-120001', eigene)).toBe(true);
		expect(neuere('20261104-120000-2', eigene)).toBe(true);
		expect(neuere('20261104-120000-10', '20261104-120000-9')).toBe(true);
	});
	it('unlesbar (Rückfall „build“, fremde Form): nein', () => {
		expect(neuere('build', eigene)).toBe(false);
		expect(neuere('x', eigene)).toBe(false);
		expect(neuere('20261104-120001', 'probe')).toBe(false);
	});
	it('SvelteKits version (Millisekunden beim Bau) gegen den Ordner des Boten', () => {
		const ms = kennungZeit(eigene);
		if (ms === null) throw new Error('unlesbar');
		// Derselbe Bau: Vite beginnt nach dem Namen des Ordners, die Kennung ist älter.
		expect(neuere(eigene, String(ms + 4000))).toBe(false);
		// Ein späterer Bau beginnt erst nach dem Ende des vorigen.
		expect(neuere('20261104-120100', String(ms + 4000))).toBe(true);
	});
	it('mit dem Abdruck des Grundbaus davor (`<16 Hex>-<ms>`) wie ohne', () => {
		const ms = kennungZeit(eigene) ?? 0;
		const tab = `0123456789abcdef-${ms + 4000}`;
		expect(kennungZeit(tab)).toBe(ms + 4000);
		expect(neuere(eigene, tab)).toBe(false);
		expect(neuere('20261104-120100', tab)).toBe(true);
		// Gegenprobe: ein anderer Vorsatz ist keine Kennung.
		expect(kennungZeit(`0123456789ABCDEF-${ms}`)).toBeNull();
		expect(kennungZeit(`abc-${ms}`)).toBeNull();
	});
});

describe('Fassung: Zustand im Tab', () => {
	const umfeld = $state<{ verbunden: boolean | null; baut: boolean }>({
		verbunden: true,
		baut: false
	});
	const test: Umfeld = { verbunden: () => umfeld.verbunden, baut: () => umfeld.baut };
	const EIGENE = '20261104-120000';
	beforeEach(() => {
		umfeld.verbunden = true;
		umfeld.baut = false;
	});

	it('gleiche oder ältere Kennung: kein Hinweis; neuere: veraltet, kein sofortiges Neuladen', () => {
		const f = new Fassung(EIGENE, test);
		f.ereignis(EIGENE);
		f.ereignis('20261104-110000');
		expect(f.veraltet).toBe(false);
		expect(f.lage).toBeNull();
		f.ereignis('20261104-120500');
		expect(f.veraltet).toBe(true);
		expect(f.lage).toBe('veraltet');
		expect(neuLaden).not.toHaveBeenCalled();
	});

	it('Ereignis vor dem ersten Laden (ein Bau, den der Tab schon hat): kein Hinweis', () => {
		// Der Tab lud den Bau 12:00:00 (Vite begann 4 s später); das Ereignis dazu kommt erst danach an.
		const ms = kennungZeit(EIGENE) ?? 0;
		const f = new Fassung(String(ms + 4000), test);
		f.ereignis(EIGENE);
		expect(f.lage).toBeNull();
	});

	it('Vorrang 1 vor 3 vor 2, und Übergang 3 nach 2 nach grünem Bau', () => {
		const f = new Fassung(EIGENE, test);
		umfeld.baut = true;
		f.ereignis('20261104-120500');
		expect(f.lage).toBe('baut');
		umfeld.verbunden = false;
		expect(f.lage).toBe('keine_verbindung');
		umfeld.verbunden = true;
		expect(f.lage).toBe('baut');
		// Bau grün (Ereignis `bau` mit `gruen`): weiter mit 2.
		umfeld.baut = false;
		expect(f.lage).toBe('veraltet');
	});

	it('roter Bau ohne neue Fassung: der Hinweis verschwindet', () => {
		const f = new Fassung(EIGENE, test);
		umfeld.baut = true;
		expect(f.lage).toBe('baut');
		umfeld.baut = false;
		expect(f.lage).toBeNull();
	});

	it('selbstLaden lädt einmal und merkt es sich; gleich danach nicht noch einmal', () => {
		expect(new Fassung('v1', test).selbstLaden()).toBe(true);
		expect(neuLaden).toHaveBeenCalledTimes(1);
		expect(merkerLesen(sessionStorage)).toEqual({ zeit: uhr, von: 'v1' });
		seiteNeu();
		uhr += 2000;
		expect(new Fassung('v2', test).selbstLaden()).toBe(false);
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('jetztLaden: Text in einer Antwort, Fokus beim Klick schon weg: kein Neuladen, aufgehalten, Fokus zurück ins Feld', () => {
		document.body.innerHTML = '<textarea id="antwort"></textarea><button id="punkt">x</button>';
		const t = document.querySelector('textarea');
		if (t === null) throw new Error('kein Feld');
		t.value = 'Halb geschrieben';
		document.querySelector<HTMLButtonElement>('#punkt')?.focus();
		const f = new Fassung('v1', test);
		f.jetztLaden();
		expect(neuLaden).not.toHaveBeenCalled();
		expect(f.aufgehalten).toBe(true);
		expect(document.activeElement).toBe(t);
		expect(t.value).toBe('Halb geschrieben');
	});

	it('jetztLaden: offener Dialog oder laufende Freigabe: kein Neuladen', () => {
		const f = new Fassung('v1', test);
		document.body.innerHTML = '<dialog open><p>Nur Text</p></dialog>';
		f.jetztLaden();
		document.body.innerHTML = '<div role="group" data-sperrt-neuladen></div>';
		f.jetztLaden();
		expect(neuLaden).not.toHaveBeenCalled();
	});

	it('jetztLaden (Klick, Abkürzung) lädt sofort, ohne Schleifensperre, wenn nichts offen ist', () => {
		new Fassung('v1', test).selbstLaden();
		seiteNeu();
		new Fassung('v2', test).jetztLaden();
		expect(neuLaden).toHaveBeenCalledTimes(2);
	});

	/** Eine Navigation von /anstehend nach `ziel` (Route `route`). */
	function navigation(
		cancel: () => void,
		ziel = 'http://localhost/pc',
		route = '/pc',
		type = 'link'
	): Navigation {
		return {
			from: { route: { id: '/anstehend' } },
			to: { url: new URL(ziel), route: { id: route } },
			willUnload: false,
			type,
			cancel
		};
	}

	it('veraltet: der nächste Klick auf eine andere Route geht am Router vorbei zum Ziel', () => {
		const f = new Fassung(EIGENE, test);
		f.ereignis('20261104-120500');
		const cancel = vi.fn();
		f.vorNavigation(navigation(cancel));
		expect(cancel).toHaveBeenCalled();
		expect(gehe).toHaveBeenCalledWith('http://localhost/pc');
	});

	it('veraltet, aber dieselbe Route (goto mit ?ansicht= und &art=): bleibt im Router', () => {
		const f = new Fassung(EIGENE, test);
		f.ereignis('20261104-120500');
		const cancel = vi.fn();
		f.vorNavigation(
			navigation(cancel, 'http://localhost/anstehend?ansicht=liste&art=frage', '/anstehend', 'goto')
		);
		expect(cancel).not.toHaveBeenCalled();
		expect(gehe).not.toHaveBeenCalled();
	});

	it('veraltet, Zurück/Vor (popstate): durchlassen, nicht abbrechen und neu gehen', () => {
		const f = new Fassung(EIGENE, test);
		f.ereignis('20261104-120500');
		const cancel = vi.fn();
		f.vorNavigation(navigation(cancel, 'http://localhost/pc', '/pc', 'popstate'));
		expect(cancel).not.toHaveBeenCalled();
		expect(gehe).not.toHaveBeenCalled();
	});

	it('veraltet, aber etwas offen (Text in einer Antwort, Dialog): keine volle Navigation', () => {
		const f = new Fassung(EIGENE, test);
		f.ereignis('20261104-120500');
		const cancel = vi.fn();
		document.body.innerHTML = '<textarea></textarea>';
		const t = document.querySelector('textarea');
		if (t === null) throw new Error('kein Feld');
		t.value = 'Halb geschrieben';
		f.vorNavigation(navigation(cancel));
		document.body.innerHTML = '<div role="dialog"><p>Schublade</p></div>';
		f.vorNavigation(navigation(cancel));
		expect(cancel).not.toHaveBeenCalled();
		expect(gehe).not.toHaveBeenCalled();
	});

	it('nicht veraltet, ohne Verbindung, im Bau, beim Verlassen und ohne Ziel bleibt die Navigation', () => {
		const f = new Fassung(EIGENE, test);
		const cancel = vi.fn();
		const nav = navigation(cancel);
		f.vorNavigation(nav);
		f.veraltet = true;
		f.vorNavigation({ ...nav, type: 'leave', willUnload: true });
		f.vorNavigation({ ...nav, to: null });
		umfeld.verbunden = false;
		f.vorNavigation(nav);
		umfeld.verbunden = true;
		umfeld.baut = true;
		f.vorNavigation(nav);
		expect(cancel).not.toHaveBeenCalled();
		expect(gehe).not.toHaveBeenCalled();
	});
});

describe('Fehlender Teil des Baus: einmal neu laden, keine Schleife', () => {
	it('lädt beim ersten Mal neu; scheitert es danach gleich wieder, nicht noch einmal', () => {
		expect(new Fassung('v1').chunkFehler()).toBe(true);
		expect(neuLaden).toHaveBeenCalledTimes(1);
		// Neu geladen, aber derselbe Teil fehlt wieder (etwa im neuen Bau selbst).
		seiteNeu();
		uhr += 500;
		expect(new Fassung('v2').chunkFehler()).toBe(false);
		seiteNeu();
		uhr += SPERRE_MS;
		// Dieselbe Fassung, die schon einmal neu lud: nicht noch einmal.
		expect(new Fassung('v1').chunkFehler()).toBe(false);
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('mehrere Fehler in derselben Seite: ein Neuladen, alle gelten als erledigt', () => {
		const f = new Fassung('v1');
		expect(f.chunkFehler()).toBe(true);
		expect(f.chunkFehler()).toBe(true);
		expect(einmalNeuLaden('v1')).toBe(true);
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('eine spätere Fassung darf wieder neu laden', () => {
		expect(new Fassung('v1').chunkFehler()).toBe(true);
		seiteNeu();
		uhr += SPERRE_MS + 1;
		expect(new Fassung('v2').chunkFehler()).toBe(true);
		expect(neuLaden).toHaveBeenCalledTimes(2);
	});

	it('ohne Verbindung (Zustand 1) nie: der Satz „keine Verbindung“ statt Neuladen', () => {
		const f = new Fassung('v1', { verbunden: () => false, baut: () => false });
		expect(f.lage).toBe('keine_verbindung');
		expect(f.chunkFehler()).toBe(false);
		expect(neuLaden).not.toHaveBeenCalled();
		expect(merkerLesen(sessionStorage)).toBeNull();
		// Im Bau oder veraltet (verbunden) lädt es weiter einmal neu.
		expect(new Fassung('v1', { verbunden: () => true, baut: () => true }).chunkFehler()).toBe(true);
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it.each([
		['Text in einer Antwort', '<textarea>Halb geschrieben</textarea>'],
		['offene Rückfrage', '<div role="group" data-sperrt-neuladen><button>Ja</button></div>']
	])('etwas offen (%s): kein Neuladen, die Seite zeigt den Fehler', (_, html) => {
		document.body.innerHTML = html;
		expect(new Fassung('v1').chunkFehler()).toBe(false);
		expect(neuLaden).not.toHaveBeenCalled();
		expect(merkerLesen(sessionStorage)).toBeNull();
	});

	it('ohne sessionStorage kein Neuladen von selbst (kein Schutz vor Schleifen)', () => {
		umgebung.speicher = () => null;
		expect(new Fassung('v1').chunkFehler()).toBe(false);
		expect(neuLaden).not.toHaveBeenCalled();
	});
});

describe('Zustand 3 ohne Anmeldung: aus bau.zustand der Projektliste (Muss 5)', () => {
	afterEach(() => {
		bauLage.liste([]);
		freierKanal.aktiv = null;
		live.connected = false;
	});
	const projekt = (slug: string, zustand: 'laeuft' | 'gruen'): ProjectSummary =>
		({ slug, bau: { zustand, letzte_gute: null, gebaut_um: null } }) as unknown as ProjectSummary;

	it('frei: läuft laut Liste ein Bau, gilt Zustand 3 (nur ja/nein, ohne Slug); danach wieder 2', () => {
		expect(freierKanal.geschuetztOk).toBe(false);
		const f = new Fassung('20261104-120000', { verbunden: () => true, baut: echtesUmfeld.baut });
		f.ereignis('20261104-120500');
		expect(f.lage).toBe('veraltet');
		bauLage.liste([projekt('probe', 'laeuft'), projekt('anderes', 'gruen')]);
		expect(bauLage.freiLaeuft).toBe(true);
		expect(echtesUmfeld.baut()).toBe(true);
		expect(f.lage).toBe('baut');
		// Die nächste Liste (von der Wache nachgeholt) nennt keinen laufenden Bau mehr.
		bauLage.liste([projekt('probe', 'gruen')]);
		expect(f.lage).toBe('veraltet');
	});

	it('frei: ein altes Ereignis bau (aus der Zeit mit Anmeldung) zählt nicht, nur die Liste', () => {
		bauLage.ereignis({ slug: 'probe', zustand: 'laeuft', meldung: '', gescheitert: false });
		expect(bauLage.laeuft).toBe(true);
		expect(echtesUmfeld.baut()).toBe(false);
	});
});

describe('Fokus nach dem Neuladen (Sollte 6)', () => {
	it('Fokus in einem Feld mit ID: nach dem Laden einmal dorthin', () => {
		document.body.innerHTML = '<main tabindex="-1"></main><input id="suche">';
		document.querySelector<HTMLInputElement>('#suche')?.focus();
		fokusMerken(document, sessionStorage);
		expect(sessionStorage.getItem(FOKUS_MERKER)).toBe('suche');
		document.body.innerHTML = '<main tabindex="-1"></main><input id="suche">';
		fokusZurueck(document, sessionStorage);
		expect(document.activeElement?.id).toBe('suche');
		expect(sessionStorage.getItem(FOKUS_MERKER)).toBeNull();
	});

	it('Fokus am Live-Punkt (Knopf): danach auf den Link am Live-Punkt', () => {
		document.body.innerHTML =
			'<main tabindex="-1"></main><span data-live-punkt><button class="live">x</button></span>';
		document.querySelector<HTMLButtonElement>('button')?.focus();
		fokusMerken(document, sessionStorage);
		expect(sessionStorage.getItem(FOKUS_MERKER)).toBe(LIVE_PUNKT);
		document.body.innerHTML =
			'<main tabindex="-1"></main><span data-live-punkt><a class="live" href="/pc">x</a></span>';
		fokusZurueck(document, sessionStorage);
		expect(document.activeElement?.tagName).toBe('A');
	});

	it('Fokus nirgends oder das Ziel fehlt: auf main; ohne Merker (gewöhnliches Laden): nichts', () => {
		document.body.innerHTML = '<main tabindex="-1"></main>';
		fokusMerken(document, sessionStorage);
		fokusZurueck(document, sessionStorage);
		expect(document.activeElement?.tagName).toBe('MAIN');
		sessionStorage.setItem(FOKUS_MERKER, 'gibt-es-nicht');
		document.body.innerHTML = '<main tabindex="-1"></main>';
		fokusZurueck(document, sessionStorage);
		expect(document.activeElement?.tagName).toBe('MAIN');
		document.body.innerHTML = '<main tabindex="-1"></main>';
		fokusZurueck(document, sessionStorage);
		expect(document.activeElement).toBe(document.body);
	});
});
