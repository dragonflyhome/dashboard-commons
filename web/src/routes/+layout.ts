import { api } from '#lib/api/client.ts';
import type { ProjectList, ProjectSummary } from '$vertrag';
import type { LayoutLoad } from './$types';

// Einzelseiten-App: alles wird im Browser gerendert, die Daten kommen zur
// Laufzeit vom Rust-Server. Darum kein Server-Rendering und kein Vorrendern.
export const ssr = false;
export const prerender = false;

/**
 * Kopfleiste, Fassung 3: die Projekte für die Seitenleiste (frei, auch ohne
 * Anmeldung). Live über denselben Schlüssel wie die Startseite
 * (`api:projects`, Ereignisse `projekt` und `inhalt`). Geht die Liste nicht
 * (Server startet neu), steht die Leiste ohne Projekte da; die Seite selbst
 * lädt trotzdem.
 *
 * `projektListe`: die ganze Antwort, damit die Startseite `/api/projects`
 * nicht ein zweites Mal holt (`await parent()` in `+page.ts`).
 */
export const load: LayoutLoad = async ({
	fetch,
	depends
}): Promise<{ leisteProjekte: ProjectSummary[] | null; projektListe: ProjectList | null }> => {
	depends('api:projects');
	try {
		const l = await api.projects(fetch);
		return { leisteProjekte: l.projects, projektListe: l };
	} catch {
		return { leisteProjekte: null, projektListe: null };
	}
};
