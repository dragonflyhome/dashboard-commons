import { api, ApiError } from '#lib/api/client.ts';
import type { AnleitungAntwort } from '$vertrag';
import type { PageLoad } from './$types';

type Daten = { anleitung: AnleitungAntwort; fehler: null } | { anleitung: null; fehler: string };

/**
 * Lädt die Anleitung (Etappe 6b) aus docs/anleitung/*.toml; ein Fehler des
 * Servers wird ein Satz. Ohne Anmeldung lädt die Seite nicht (Schritt E2:
 * `+layout.ts` leitet zur Anmeldung).
 */
export const load: PageLoad = async ({ fetch, depends }): Promise<Daten> => {
	depends('api:anleitung');
	try {
		return { anleitung: await api.anleitung(fetch), fehler: null };
	} catch (e) {
		if (e instanceof ApiError) return { anleitung: null, fehler: e.message };
		throw e;
	}
};
