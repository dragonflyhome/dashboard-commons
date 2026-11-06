import { api, ApiError } from '#lib/api/client.ts';
import { SATZ_ANLEITUNG_ANMELDEN, SATZ_FREMDES_KONTO } from '#lib/anmeldung.ts';
import type { AnleitungAntwort } from '$vertrag';
import type { PageLoad } from './$types';

type Daten =
	| { anleitung: AnleitungAntwort; fehler: null; anmelden: null }
	| { anleitung: null; fehler: string; anmelden: null }
	| { anleitung: null; fehler: null; anmelden: string };

/**
 * Lädt die Anleitung (Etappe 6b) aus docs/anleitung/*.toml. Ohne
 * Windows-Anmeldung (401) der Satz mit dem Weg über Edge, wie beim Zielbild;
 * ein anderer Fehler wird ein Satz.
 */
export const load: PageLoad = async ({ fetch, depends }): Promise<Daten> => {
	depends('api:anleitung');
	try {
		return { anleitung: await api.anleitung(fetch), fehler: null, anmelden: null };
	} catch (e) {
		if (e instanceof ApiError && e.status === 401)
			return { anleitung: null, fehler: null, anmelden: SATZ_ANLEITUNG_ANMELDEN };
		if (e instanceof ApiError && e.art === 'fremdes-konto')
			return { anleitung: null, fehler: null, anmelden: SATZ_FREMDES_KONTO };
		if (e instanceof ApiError) return { anleitung: null, fehler: e.message, anmelden: null };
		throw e;
	}
};
