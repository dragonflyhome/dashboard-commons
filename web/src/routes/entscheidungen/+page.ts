import { alsLadeFehler, api, ApiError } from '#lib/api/client.ts';
import { SATZ_ENTSCHEIDUNGEN_ANMELDEN, SATZ_FREMDES_KONTO } from '#lib/anmeldung.ts';
import type { EntscheidungsListe } from '$vertrag';
import type { PageLoad } from './$types';

type Daten =
	| { liste: EntscheidungsListe; fehler: null; anmelden: null }
	| { liste: null; fehler: string; anmelden: null }
	| { liste: null; fehler: null; anmelden: string };

/**
 * Lädt die Entscheidungen (Etappe 4d). Ohne Windows-Anmeldung (401) der Satz
 * mit dem Weg über Edge, wie bei /anstehend; ein anderer Fehler wird ein Satz.
 */
export const load: PageLoad = async ({ fetch, depends }): Promise<Daten> => {
	depends('api:entscheidungen');
	try {
		return { liste: await api.entscheidungen(fetch), fehler: null, anmelden: null };
	} catch (e) {
		if (e instanceof ApiError && e.status === 401)
			return { liste: null, fehler: null, anmelden: SATZ_ENTSCHEIDUNGEN_ANMELDEN };
		if (e instanceof ApiError && e.art === 'fremdes-konto')
			return { liste: null, fehler: null, anmelden: SATZ_FREMDES_KONTO };
		if (e instanceof ApiError) return { liste: null, fehler: e.message, anmelden: null };
		alsLadeFehler(e);
	}
};
