import { alsLadeFehler, api, ApiError } from '#lib/api/client.ts';
import { SATZ_FREMDES_KONTO, SATZ_ZIELBILD_ANMELDEN } from '#lib/anmeldung.ts';
import type { ZielbildAntwort } from '$vertrag';
import type { PageLoad } from './$types';

type Daten =
	| { zielbild: ZielbildAntwort; fehler: null; anmelden: null }
	| { zielbild: null; fehler: string; anmelden: null }
	| { zielbild: null; fehler: null; anmelden: string };

/**
 * Lädt das Zielbild (Etappe 4d). Ohne Windows-Anmeldung (401) der Satz mit
 * dem Weg über Edge, wie bei /anstehend; ein anderer Fehler wird ein Satz.
 */
export const load: PageLoad = async ({ fetch, depends }): Promise<Daten> => {
	depends('api:zielbild');
	try {
		return { zielbild: await api.zielbild(fetch), fehler: null, anmelden: null };
	} catch (e) {
		if (e instanceof ApiError && e.status === 401)
			return { zielbild: null, fehler: null, anmelden: SATZ_ZIELBILD_ANMELDEN };
		if (e instanceof ApiError && e.art === 'fremdes-konto')
			return { zielbild: null, fehler: null, anmelden: SATZ_FREMDES_KONTO };
		if (e instanceof ApiError) return { zielbild: null, fehler: e.message, anmelden: null };
		alsLadeFehler(e);
	}
};
