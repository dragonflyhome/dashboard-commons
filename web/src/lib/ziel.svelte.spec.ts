/**
 * Etappe 3b-2 (Fehler „Anker nur beim ersten Mal“): `gleicherAnkerKlick`
 * erkennt nur den Klick auf einen Link zur aktuellen Seite, dessen Anker
 * schon in der Adresse steht und auf einen Eintrag zeigt.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { gleicherAnkerKlick } from './ziel.ts';

/** Ein Klick auf einen neuen Link mit `href` (ohne ihn auszulösen). */
function klick(href: string, teil: MouseEventInit = {}, attr: Record<string, string> = {}) {
	const a = document.createElement('a');
	a.href = href;
	for (const [k, v] of Object.entries(attr)) a.setAttribute(k, v);
	const innen = document.createElement('span');
	a.append(innen);
	document.body.append(a);
	const ev = new MouseEvent('click', { bubbles: true, button: 0, ...teil });
	Object.defineProperty(ev, 'target', { value: innen });
	return gleicherAnkerKlick(ev);
}

afterEach(() => {
	document.body.innerHTML = '';
	history.replaceState(null, '', '/');
});

describe('gleicherAnkerKlick', () => {
	it('derselbe Eintrags-Anker auf derselben Seite: der Anker', () => {
		history.replaceState(null, '', '/projects/x#frage-farbe');
		expect(klick('/projects/x#frage-farbe')).toBe('frage-farbe');
		expect(klick('#frage-farbe')).toBe('frage-farbe');
	});

	it('anderer Anker, andere Seite, kein Eintrag, Taste oder neues Fenster: null', () => {
		history.replaceState(null, '', '/projects/x#frage-farbe');
		expect(klick('/projects/x#arbeit-1')).toBeNull();
		expect(klick('/projects/y#frage-farbe')).toBeNull();
		expect(klick('/projects/x?a=1#frage-farbe')).toBeNull();
		expect(klick('/projects/x#frage-farbe', { ctrlKey: true })).toBeNull();
		expect(klick('/projects/x#frage-farbe', { metaKey: true })).toBeNull();
		expect(klick('/projects/x#frage-farbe', { button: 1 })).toBeNull();
		expect(klick('/projects/x#frage-farbe', {}, { target: '_blank' })).toBeNull();
		history.replaceState(null, '', '/projects/x#oben');
		expect(klick('/projects/x#oben')).toBeNull();
	});
});
