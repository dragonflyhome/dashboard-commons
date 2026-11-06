/**
 * Leere Abschnitte (Prüfbericht: eine Karte „Frage an …“ mit nur
 * „Keine Fragen.“). Ein Baustein, der nichts zu zeigen hat, setzt an seinen
 * Satz `data-leer`; `Abschnitt` versteckt sich, wenn sein Inhalt nur aus
 * solchen Elementen besteht.
 */

const ELEMENT = 1;
const TEXT = 3;

/** Besteht der Inhalt von `el` aus mindestens einem Element, und tragen alle `data-leer` (ohne Text daneben)? */
export function nurLeeres(el: Element): boolean {
	let elemente = 0;
	for (const n of el.childNodes) {
		if (n.nodeType === TEXT) {
			if ((n.textContent ?? '').trim() !== '') return false;
		} else if (n.nodeType === ELEMENT) {
			if (!(n as Element).hasAttribute('data-leer')) return false;
			elemente += 1;
		}
		// Kommentare (Anker von Svelte) zählen nicht.
	}
	return elemente > 0;
}
