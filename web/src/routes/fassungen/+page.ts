import { api, ApiError } from '#lib/api/client.ts';
import type { FassungenAntwort } from '$vertrag';
import type { PageLoad } from './$types';

type Daten = { fassungen: FassungenAntwort; fehler: null } | { fassungen: null; fehler: string };

/**
 * Lädt den Änderungsverlauf (`/api/fassungen`, frei: das Changelog ist
 * öffentlich). Ein Fehler der API wird ein Satz.
 */
export const load: PageLoad = async ({ fetch }): Promise<Daten> => {
	try {
		return { fassungen: await api.fassungen(fetch), fehler: null };
	} catch (e) {
		if (e instanceof ApiError) return { fassungen: null, fehler: e.message };
		throw e;
	}
};
