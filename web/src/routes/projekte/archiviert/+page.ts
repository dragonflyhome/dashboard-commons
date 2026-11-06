import { alsLadeFehler, api, ApiError } from '#lib/api/client.ts';
import type { ArchivListe } from '$vertrag';
import type { PageLoad } from './$types';

type Daten = { liste: ArchivListe; fehler: null } | { liste: null; fehler: string };

/**
 * Die Archivansicht (Entscheidung archiv-status): die
 * archivierten Projekte aus `/api/archiv` (frei wie `/api/projects`). Live
 * über `api:projects` wie die Projektliste: Ändert ein Chat den Status, lädt
 * die Seite mit. Ein Fehler wird ein Satz.
 */
export const load: PageLoad = async ({ fetch, depends }): Promise<Daten> => {
	depends('api:projects');
	try {
		return { liste: await api.archiv(fetch), fehler: null };
	} catch (e) {
		if (e instanceof ApiError) return { liste: null, fehler: e.message };
		alsLadeFehler(e);
	}
};
