/**
 * Etappe 3b: Thema. Das Skript in app.html (vor dem ersten Bild) und
 * #lib/thema.ts müssen dieselben Schlüssel und Werte nehmen; der Test führt
 * das Skript aus app.html in jsdom aus. Die Prüfsumme für die CSP hält
 * src/sicherheit.rs fest (thema_skript_der_vorlage).
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	THEMA_SCHLUESSEL,
	THEMEN,
	istThema,
	themaAnwenden,
	themaLesen,
	themaSetzen
} from './thema.ts';

const APP_HTML = readFileSync(join(import.meta.dirname, '..', 'app.html'), 'utf8');

/** Die eingebetteten Skripte der Vorlage (ohne src). */
function skripte(html: string): string[] {
	return [...html.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map(
		(m) => m[1] ?? ''
	);
}

function vorlageAusfuehren(): void {
	const [skript] = skripte(APP_HTML);
	if (skript === undefined) throw new Error('kein Skript in app.html');
	// Wie der Browser: das Skript im globalen Bereich, ohne Rückgabe.
	// eslint-disable-next-line @typescript-eslint/no-implied-eval, @typescript-eslint/no-unsafe-call -- führt genau das Skript aus app.html aus, wie der Browser es täte; nur im Test
	new Function(skript)();
}

function metas(): HTMLMetaElement[] {
	document.head.innerHTML =
		'<meta name="theme-color" content="a" media="(prefers-color-scheme: light)">' +
		'<meta name="theme-color" content="b" media="(prefers-color-scheme: dark)">';
	return [...document.head.querySelectorAll('meta')];
}

afterEach(() => {
	localStorage.clear();
	document.documentElement.removeAttribute('data-theme');
	vi.restoreAllMocks();
});

describe('Skript in app.html (kein Aufblitzen)', () => {
	it('ist das einzige eingebettete Skript, steht im head und nimmt den Schlüssel aus thema.ts', () => {
		const s = skripte(APP_HTML);
		expect(s).toHaveLength(1);
		expect(s[0]).toContain(`'${THEMA_SCHLUESSEL}'`);
		const kopf = APP_HTML.slice(0, APP_HTML.indexOf('</head>'));
		expect(kopf).toContain('<script>');
		expect(APP_HTML.indexOf('<script>')).toBeLessThan(APP_HTML.indexOf('%sveltekit.head%'));
	});

	it.each(['hell', 'dunkel'] as const)('gewählt „%s“: setzt data-theme', (t) => {
		localStorage.setItem(THEMA_SCHLUESSEL, t);
		vorlageAusfuehren();
		expect(document.documentElement.getAttribute('data-theme')).toBe(t);
		expect(themaLesen()).toBe(t);
	});

	it('ohne Wahl oder mit fremdem Wert: kein data-theme (System)', () => {
		vorlageAusfuehren();
		expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
		localStorage.setItem(THEMA_SCHLUESSEL, 'lila');
		vorlageAusfuehren();
		expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
		expect(themaLesen()).toBe('system');
	});

	/**
	 * Etappe S1: Das Skript kennt seine Werte von Hand (es kann nichts
	 * importieren, und die CSP erlaubt es nur über seine Prüfsumme). Darum
	 * hält dieser Test es an der Liste fest: Jeder Wert aus THEMEN außer
	 * `system` muss dort ankommen.
	 */
	it.each(THEMEN.map((t) => t.wert).filter((w) => w !== 'system'))(
		'Wert „%s“ aus THEMEN: das Skript setzt data-theme',
		(wert) => {
			localStorage.setItem(THEMA_SCHLUESSEL, wert);
			vorlageAusfuehren();
			expect(document.documentElement.getAttribute('data-theme')).toBe(wert);
			expect(themaLesen()).toBe(wert);
		}
	);

	it('ein Wert, der nicht in THEMEN steht: das Skript setzt kein data-theme', () => {
		const fremd = 'kein-thema-der-liste';
		expect(THEMEN.some((t) => (t.wert as string) === fremd)).toBe(false);
		localStorage.setItem(THEMA_SCHLUESSEL, fremd);
		vorlageAusfuehren();
		expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
		expect(themaLesen()).toBe('system');
	});

	it('gesperrter Speicher: kein Fehler, System', () => {
		vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
			throw new Error('gesperrt');
		});
		vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
			throw new Error('gesperrt');
		});
		expect(() => {
			vorlageAusfuehren();
		}).not.toThrow();
		expect(themaLesen()).toBe('system');
		expect(() => {
			themaSetzen('dunkel');
		}).not.toThrow();
		// Gilt trotzdem bis zum Neuladen.
		expect(document.documentElement.getAttribute('data-theme')).toBe('dunkel');
	});
});

describe('thema.ts', () => {
	it('merkt die Wahl; System löscht sie', () => {
		themaSetzen('hell');
		expect(localStorage.getItem(THEMA_SCHLUESSEL)).toBe('hell');
		expect(document.documentElement.getAttribute('data-theme')).toBe('hell');
		themaSetzen('system');
		expect(localStorage.getItem(THEMA_SCHLUESSEL)).toBeNull();
		expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
	});

	it('Farbe der Titelleiste folgt der Wahl und kehrt bei System zum System zurück', () => {
		const [hell, dunkel] = metas();
		const media = (): (string | null | undefined)[] => [
			hell?.getAttribute('media'),
			dunkel?.getAttribute('media')
		];
		themaAnwenden('dunkel');
		expect(media()).toEqual(['not all', 'all']);
		themaAnwenden('hell');
		expect(media()).toEqual(['all', 'not all']);
		themaAnwenden('system');
		expect(media()).toEqual(['(prefers-color-scheme: light)', '(prefers-color-scheme: dark)']);
	});
});

describe('Etappe S1: die Liste THEMEN ist die eine Quelle', () => {
	it('istThema kennt genau die Werte der Liste', () => {
		for (const { wert } of THEMEN) expect(istThema(wert)).toBe(true);
		for (const fremd of ['lila', '', 'HELL', ' hell', null, undefined, 0, {}]) {
			expect(istThema(fremd)).toBe(false);
		}
	});

	it('themaLesen gibt jeden gemerkten Wert der Liste zurück, sonst system', () => {
		for (const { wert } of THEMEN) {
			localStorage.setItem(THEMA_SCHLUESSEL, wert);
			expect(themaLesen()).toBe(wert);
		}
		localStorage.setItem(THEMA_SCHLUESSEL, 'kein-thema-der-liste');
		expect(themaLesen()).toBe('system');
		localStorage.clear();
		expect(themaLesen()).toBe('system');
	});

	it('jeder Eintrag hat einen eigenen Wert und einen eigenen Namen', () => {
		expect(new Set(THEMEN.map((t) => t.wert)).size).toBe(THEMEN.length);
		expect(new Set(THEMEN.map((t) => t.text)).size).toBe(THEMEN.length);
		expect(THEMEN.some((t) => t.wert === 'system')).toBe(true);
	});
});
