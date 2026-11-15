/**
 * Für Tests der Seiten (Schritt E2): Wohin leitet das Layout eine Seite ohne
 * Anmeldung? Ruft `load` aus `+layout.ts` mit `zugang` „fehlt“; gibt das Ziel
 * der Umleitung oder `null` (die Seite lädt). Nur in Komponenten-Tests (jsdom):
 * `redirect` braucht `window`.
 */
import { isRedirect } from '@sveltejs/kit';
import { zugang } from '#lib/zugang.ts';
import { load } from './+layout.ts';

export async function leitetZurAnmeldung(pfad: string): Promise<string | null> {
	zugang.setzen('fehlt');
	try {
		await load({
			url: new URL(pfad, 'http://localhost:8787'),
			fetch: () => Promise.reject(new Error('ohne Anmeldung kein Abruf')),
			depends: () => undefined
		} as never);
		return null;
	} catch (e) {
		if (isRedirect(e)) return e.location;
		throw e;
	} finally {
		zugang.vergessen();
	}
}
