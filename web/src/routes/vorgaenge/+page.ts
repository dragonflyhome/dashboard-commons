import { alsLadeFehler, AnmeldungNoetig, api, ApiError } from '#lib/api/client.ts';
import type { VorgangsUebersicht } from '$vertrag';
import type { PageLoad } from './$types';

type Daten =
	{ uebersicht: VorgangsUebersicht; fehler: null } | { uebersicht: null; fehler: string };

/**
 * Lädt die Übersicht. Fehler des Servers (etwa 503, wenn der Datenspeicher
 * fehlt) werden nicht geworfen, sondern als Satz gezeigt. Fehlt die
 * Windows-Anmeldung (401) oder ist es ein fremdes Konto (403), zeigt die
 * Fehlerseite den Weg über localhost.
 */
export const load: PageLoad = async ({ fetch, depends }): Promise<Daten> => {
	depends('api:vorgaenge');
	try {
		return { uebersicht: await api.vorgaengeUebersicht(fetch), fehler: null };
	} catch (e) {
		if (e instanceof AnmeldungNoetig || (e instanceof ApiError && e.art === 'fremdes-konto')) {
			alsLadeFehler(e);
		}
		if (e instanceof ApiError) return { uebersicht: null, fehler: e.message };
		alsLadeFehler(e);
	}
};
