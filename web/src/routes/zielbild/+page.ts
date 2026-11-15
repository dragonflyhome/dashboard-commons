import { alsLadeFehler, api, ApiError } from '#lib/api/client.ts';
import type { ZielbildAntwort } from '$vertrag';
import type { PageLoad } from './$types';

type Daten = { zielbild: ZielbildAntwort; fehler: null } | { zielbild: null; fehler: string };

/**
 * Lädt das Zielbild (Etappe 4d); ein Fehler des Servers wird ein Satz. Ohne
 * Anmeldung lädt die Seite nicht (Schritt E2: `+layout.ts` leitet zur Anmeldung).
 */
export const load: PageLoad = async ({ fetch, depends }): Promise<Daten> => {
	depends('api:zielbild');
	try {
		return { zielbild: await api.zielbild(fetch), fehler: null };
	} catch (e) {
		if (e instanceof ApiError) return { zielbild: null, fehler: e.message };
		alsLadeFehler(e);
	}
};
