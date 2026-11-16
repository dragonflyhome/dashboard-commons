/**
 * Etappe 3b: Thema hell oder dunkel, seit Etappe S2 gewählt auf der Seite
 * Einstellungen (Wie mein PC, Hell, Dunkel). Die Wahl gilt je Browser
 * (localStorage, Entscheidung plan-3b.toml, Antwort 2); ohne Wahl folgt das
 * Thema dem System.
 *
 * `data-theme` am <html> trägt die Wahl: `hell` oder `dunkel`, bei System
 * fehlt es. app.css hat dafür eigene Blöcke mit denselben Werten wie das
 * System-Thema. Beim Laden setzt das kleine Skript in app.html das Attribut,
 * bevor die Seite zum ersten Mal gezeichnet wird; Schlüssel und Werte müssen
 * dort gleich sein (thema.spec.ts führt das Skript aus und vergleicht).
 *
 * Jeder Zugriff auf den Speicher steht in try/catch: Im privaten Fenster oder
 * mit gesperrtem Speicher gilt einfach System.
 */

/** Schlüssel im localStorage, derselbe wie im Skript in app.html. */
export const THEMA_SCHLUESSEL = 'dashboard-thema';

/**
 * Die eine Liste der Themen (Etappe S1): Typ, Prüfung, gemerkte Wahl und die
 * Optionen des Schalters kommen von hier. `system` ist die fehlende Wahl; jeder
 * andere Wert braucht in app.css seinen Block `:root[data-theme='<wert>'],
 * [data-thema-vorschau='<wert>']` (kontrast.spec.ts verlangt ihn) und muss dem
 * Skript in app.html bekannt sein (thema.svelte.spec.ts führt es je Wert aus).
 */
export const THEMEN = [
	{ wert: 'system', text: 'Wie mein PC' },
	{ wert: 'hell', text: 'Hell' },
	{ wert: 'dunkel', text: 'Dunkel' }
] as const;

export type Thema = (typeof THEMEN)[number]['wert'];

export function istThema(x: unknown): x is Thema {
	return THEMEN.some((t) => t.wert === x);
}

/** Die gespeicherte Wahl; ohne Wahl, mit fremdem Wert oder ohne Speicher `system`. */
export function themaLesen(): Thema {
	try {
		const t = localStorage.getItem(THEMA_SCHLUESSEL);
		return istThema(t) ? t : 'system';
	} catch {
		return 'system';
	}
}

/**
 * Setzt `data-theme` und die Farbe der Titelleiste (theme-color, für die
 * angeheftete Edge-App): bei einer Wahl gilt nur das passende meta, bei System
 * wieder beide nach dem System.
 */
export function themaAnwenden(t: Thema, doc: Document = document): void {
	const wurzel = doc.documentElement;
	if (t === 'system') wurzel.removeAttribute('data-theme');
	else wurzel.setAttribute('data-theme', t);
	for (const meta of doc.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')) {
		// Welches Thema das meta meint, steht beim ersten Mal in seiner media-Angabe.
		const fuer =
			meta.getAttribute('data-thema') ??
			((meta.getAttribute('media') ?? '').includes('dark') ? 'dunkel' : 'hell');
		meta.setAttribute('data-thema', fuer);
		const system =
			fuer === 'dunkel' ? '(prefers-color-scheme: dark)' : '(prefers-color-scheme: light)';
		meta.setAttribute('media', t === 'system' ? system : t === fuer ? 'all' : 'not all');
	}
}

/** Merkt die Wahl (System löscht sie) und wendet sie an. */
export function themaSetzen(t: Thema, doc: Document = document): void {
	try {
		if (t === 'system') localStorage.removeItem(THEMA_SCHLUESSEL);
		else localStorage.setItem(THEMA_SCHLUESSEL, t);
	} catch {
		// Speicher gesperrt: gilt nur bis zum Neuladen.
	}
	themaAnwenden(t, doc);
}
