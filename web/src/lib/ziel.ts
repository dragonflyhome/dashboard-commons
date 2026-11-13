/**
 * Ziele und Anker (Etappe A1, Konzept [ziele]): Jeder Eintrag auf der
 * Projektseite trägt die id `<art>-<id>`. Kommt man über
 * `/projects/<slug>#<art>-<id>`, klappt der Eintrag auf (falls einklappbar),
 * wird ruhig hervorgehoben und bekommt den Fokus. Nur die Navigation des
 * Dashboards selbst, kein neues Fenster, keine Benachrichtigungen.
 */

/** Klasse der Hervorhebung (in app.css, nur Token). */
export const HERVORGEHOBEN = 'ziel-hervorgehoben';

/** Was ein Anker auf der Projektseite ansteuern kann. */
export type ZielArt = 'aufgabe' | 'frage' | 'arbeit' | 'meldung' | 'etappe';

/**
 * Ob ein Anker auf einen Eintrag zeigt (`aufgabe-…`, `frage-…`, `arbeit-…`,
 * seit A3 `meldung-…`, seit B8 `etappe-…` im Plan).
 */
export function ankerZumEintrag(hash: string): { art: ZielArt } | null {
	const m = /^(aufgabe|frage|arbeit|meldung|etappe)-./.exec(hash);
	const art = m?.[1];
	return art === 'aufgabe' ||
		art === 'frage' ||
		art === 'arbeit' ||
		art === 'meldung' ||
		art === 'etappe'
		? { art }
		: null;
}

/** Der Anker aus `location.hash` oder `page.url.hash`, ohne `#`. */
export function ankerAus(hash: string): string {
	try {
		return decodeURIComponent(hash.replace(/^#/, ''));
	} catch {
		return '';
	}
}

/**
 * Steuert den Eintrag mit dieser id an: Vorfahren und ihn selbst aufklappen
 * (`<details>`), hervorheben, sichtbar machen (ohne Animation bei
 * prefers-reduced-motion) und fokussieren. `false`, wenn es ihn nicht gibt.
 */
export function ansteuern(id: string, doc: Document = document): boolean {
	const el = doc.getElementById(id);
	if (el === null) return false;
	for (let p: HTMLElement | null = el; p !== null; p = p.parentElement) {
		if (p instanceof HTMLDetailsElement) p.open = true;
	}
	for (const alt of doc.querySelectorAll(`.${HERVORGEHOBEN}`)) alt.classList.remove(HERVORGEHOBEN);
	el.classList.add(HERVORGEHOBEN);
	if (!el.hasAttribute('tabindex')) el.setAttribute('tabindex', '-1');
	const ruhig = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
	// jsdom kennt scrollIntoView nicht; im Browser gibt es die Methode immer.
	if ('scrollIntoView' in el && typeof el.scrollIntoView === 'function') {
		el.scrollIntoView({ behavior: ruhig ? 'auto' : 'smooth', block: 'center' });
	}
	el.focus({ preventScroll: true });
	return true;
}

/**
 * Etappe 3b-2 (Fehler „Anker nur beim ersten Mal“): Ein Klick auf einen Link
 * zur aktuellen Seite, dessen Anker schon in der Adresse steht, löst kein
 * `hashchange` aus; `page.url.hash` bleibt gleich, und ein inzwischen von
 * Hand zugeklappter Eintrag bliebe zu. Diese Funktion erkennt genau diesen
 * Klick und gibt den Anker (ohne `#`) zurück, wenn er auf einen Eintrag zeigt
 * (`ankerZumEintrag`); sonst `null`. Nur der linke Knopf ohne Strg, Umschalt,
 * Alt oder Meta und nur Links im selben Fenster. Ein Link mit anderem Anker
 * ist `null`: Den steuert der Effekt über `page.url.hash` an, nicht doppelt.
 */
export function gleicherAnkerKlick(ev: MouseEvent, ort: Location = location): string | null {
	if (ev.button !== 0 || ev.ctrlKey || ev.shiftKey || ev.altKey || ev.metaKey) return null;
	const ziel = ev.target;
	const a = ziel instanceof Element ? ziel.closest('a[href]') : null;
	if (!(a instanceof HTMLAnchorElement)) return null;
	if ((a.target !== '' && a.target !== '_self') || a.hasAttribute('download')) return null;
	let url: URL;
	try {
		url = new URL(a.href, ort.href);
	} catch {
		return null;
	}
	if (url.origin !== ort.origin || url.pathname !== ort.pathname || url.search !== ort.search)
		return null;
	if (url.hash === '' || url.hash !== ort.hash) return null;
	const id = ankerAus(url.hash);
	return ankerZumEintrag(id) === null ? null : id;
}
