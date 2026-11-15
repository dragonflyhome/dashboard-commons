/**
 * Schritt E2b: Ein Teil des Baus fehlt. Mit 401 (Anmeldung abgelaufen oder
 * widerrufen) geht es zur Anmeldung, nie zum Neuladen (bei offener Eingabe:
 * „abgelaufen“, die Seite bleibt); nur mit gültiger
 * Anmeldung gilt es als Teil eines alten Baus.
 */
import { describe, expect, it, vi } from 'vitest';

// Vitest kennt keine Fassung des Baus (fassung.svelte.ts liest sie beim Laden).
vi.mock('$app/env', () => ({ version: 'probe', browser: true, building: false, dev: false }));
import { eingabeDa, eingabeGeaendert, fokusZeigen, teilFehlt, type Hilfen } from './ladefehler.ts';
import { rollenBeobachten } from './components/projekt/probe/rollen.ts';
import type { Zugangsstand } from './zugang.ts';

const CHUNK = new TypeError(
	'Failed to fetch dynamically imported module: http://localhost:8787/_app/immutable/chunks/x.js'
);

function hilfen(stand: Zugangsstand, neu = true, offen = false) {
	const reihe: string[] = [];
	const h = {
		zugang: {
			vergessen: vi.fn(() => {
				reihe.push('vergessen');
			}),
			pruefen: vi.fn(() => {
				reihe.push('pruefen');
				return Promise.resolve(stand);
			})
		},
		zurAnmeldung: vi.fn<(a: string) => void>(),
		neuLaden: vi.fn(() => neu),
		offen: () => offen,
		ort: () => ({ pathname: '/projects/x/plan', search: '?a=1' })
	} satisfies Hilfen;
	return { h, reihe };
}

describe('teilFehlt', () => {
	it('kein Teil des Baus: nichts fragen, das Fehlerbild bleibt dem Aufrufer', async () => {
		const { h } = hilfen('fehlt');
		expect(await teilFehlt(new Error('kaputt'), h)).toBe('fehler');
		expect(h.zugang.pruefen).not.toHaveBeenCalled();
		expect(h.zurAnmeldung).not.toHaveBeenCalled();
		expect(h.neuLaden).not.toHaveBeenCalled();
	});

	it.each(['fehlt', 'fremd'] as const)(
		'ohne Anmeldung (%s): zur Anmeldung mit dem Weg zurück, kein Neuladen',
		async (stand) => {
			const { h, reihe } = hilfen(stand);
			expect(await teilFehlt(CHUNK, h)).toBe('weiter');
			expect(reihe).toEqual(['vergessen', 'pruefen']);
			expect(h.zurAnmeldung).toHaveBeenCalledWith(
				'/anmelden?ziel=%2Fprojects%2Fx%2Fplan%3Fa%3D1&umgeleitet=1'
			);
			expect(h.neuLaden).not.toHaveBeenCalled();
		}
	);

	it.each([
		['fehlt', 'abgelaufen'],
		['fremd', 'fremd']
	] as const)(
		'ohne Anmeldung (%s), aber etwas Ungesichertes offen: %s, die Seite bleibt stehen',
		async (stand, wie) => {
			const { h } = hilfen(stand, true, true);
			expect(await teilFehlt(CHUNK, h)).toBe(wie);
			expect(h.zurAnmeldung).not.toHaveBeenCalled();
			expect(h.neuLaden).not.toHaveBeenCalled();
		}
	);

	it.each(['da', 'unbekannt'] as const)(
		'mit Anmeldung (%s): ein Teil eines alten Baus, wie bisher',
		async (stand) => {
			const { h } = hilfen(stand, false);
			expect(await teilFehlt(CHUNK, h)).toBe('fehler');
			expect(h.neuLaden).toHaveBeenCalledTimes(1);
			expect(h.zurAnmeldung).not.toHaveBeenCalled();
			// Lädt sie neu, zeigt der Aufrufer kein Fehlerbild.
			const neu = hilfen(stand, true).h;
			expect(await teilFehlt(CHUNK, neu)).toBe('weiter');
			expect(neu.neuLaden).toHaveBeenCalledTimes(1);
			// Etwas offen: kein Neuladen, das Fehlerbild (der Rahmen lässt die Seite stehen).
			const offen = hilfen(stand, true, true).h;
			expect(await teilFehlt(CHUNK, offen)).toBe('fehler');
			expect(offen.neuLaden).not.toHaveBeenCalled();
		}
	);
});

describe('eingabeDa: auch Felder ohne Formular, solange die Anmeldung fehlt', () => {
	/** Das einzige Element wahl (ohne !, das der Lint verbietet). */
	function eins<K extends keyof HTMLElementTagNameMap>(
		d: Document,
		wahl: K
	): HTMLElementTagNameMap[K] {
		const e = d.querySelector(wahl);
		if (e === null) throw new Error(wahl);
		return e;
	}

	function seite(html: string): Document {
		document.body.innerHTML = html;
		return document;
	}

	it('unverändert: nichts offen', () => {
		const d = seite(
			'<input value="vorbelegt"><input type="file"><textarea></textarea><select><option>a</option><option>b</option></select><select><option>a</option><option selected>b</option></select><input type="checkbox" checked><div contenteditable="true"> </div><button>x</button>'
		);
		expect(eingabeGeaendert(d)).toBe(false);
		expect(eingabeDa(d)).toBe(false);
	});

	it.each([
		[
			'Eingabefeld ohne Formular',
			'<input>',
			(d: Document): void => {
				eins(d, 'input').value = 'Notiz';
			}
		],
		[
			'Auswahl',
			'<select><option>a</option><option>b</option></select>',
			(d: Document): void => {
				eins(d, 'select').selectedIndex = 1;
			}
		],
		[
			'Häkchen',
			'<input type="checkbox">',
			(d: Document): void => {
				eins(d, 'input').checked = true;
			}
		],
		[
			'gewählte Datei',
			'<input type="file">',
			(d: Document): void => {
				// jsdom lässt `files` nicht setzen; der Browser füllt es bei der Wahl.
				Object.defineProperty(eins(d, 'input'), 'files', { value: [new File(['x'], 'a.txt')] });
			}
		],
		[
			'contenteditable',
			'<div contenteditable="true"></div>',
			(d: Document): void => {
				eins(d, 'div').textContent = 'Text';
			}
		]
	] as const)('%s geändert: offen, ohne Fokus und ohne Formular', (_, html, aendern) => {
		const d = seite(html);
		aendern(d);
		expect(eingabeGeaendert(d)).toBe(true);
		expect(eingabeDa(d)).toBe(true);
	});
});

describe('fokusZeigen', () => {
	it('ohne Fokus (body) rollt nichts, mit Fokus das Feld', async () => {
		const { rollen, ende } = rollenBeobachten();
		try {
			(document.activeElement as HTMLElement | null)?.blur();
			expect(document.activeElement).toBe(document.body);
			await fokusZeigen();
			expect(rollen).not.toHaveBeenCalled();
			const feld = document.body.appendChild(document.createElement('input'));
			feld.focus();
			await fokusZeigen();
			expect(rollen.mock.contexts).toEqual([feld]);
			feld.remove();
		} finally {
			ende();
		}
	});
});
