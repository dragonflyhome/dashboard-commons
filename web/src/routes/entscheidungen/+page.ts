import { alsLadeFehler, api, ApiError } from '#lib/api/client.ts';
import type { EntscheidungsListe } from '$vertrag';
import type { PageLoad } from './$types';

type Daten = { liste: EntscheidungsListe; fehler: null } | { liste: null; fehler: string };

/**
 * Lädt die Entscheidungen (Etappe 4d); ein Fehler des Servers wird ein Satz. Ohne
 * Anmeldung lädt die Seite nicht (Schritt E2: `+layout.ts` leitet zur Anmeldung).
 */
export const load: PageLoad = async ({ fetch, depends }): Promise<Daten> => {
	depends('api:entscheidungen');
	try {
		return { liste: await api.entscheidungen(fetch), fehler: null };
	} catch (e) {
		if (e instanceof ApiError) return { liste: null, fehler: e.message };
		alsLadeFehler(e);
	}
};
