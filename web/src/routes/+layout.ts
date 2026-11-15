import { redirect } from '@sveltejs/kit';
import { api, ApiError } from '#lib/api/client.ts';
import { anmeldenAdresse, OEFFENTLICHE_SEITEN, zugang } from '#lib/zugang.ts';
import type { ProjectList, ProjectSummary } from '$vertrag';
import type { LayoutLoad } from './$types';

// Einzelseiten-App: alles wird im Browser gerendert, die Daten kommen zur
// Laufzeit vom Rust-Server. Darum kein Server-Rendering und kein Vorrendern.
export const ssr = false;
export const prerender = false;

interface Daten {
	leisteProjekte: ProjectSummary[] | null;
	projektListe: ProjectList | null;
}

const LEER: Daten = { leisteProjekte: null, projektListe: null };

/**
 * Schritt E2, die eine Stelle für die Anmeldung: Ohne Anmeldung (`zugang`
 * „fehlt“ oder „fremd“) lädt nur eine Seite aus `OEFFENTLICHE_SEITEN`; jede
 * andere geht zu `/anmelden?ziel=<pfad>&umgeleitet=1`. Den Pfad liest `load` nur dann:
 * So läuft es ohne Anmeldung bei jedem Seitenwechsel neu, angemeldet nicht.
 */
function zurAnmeldung(url: URL): Daten {
	if (!OEFFENTLICHE_SEITEN.has(url.pathname)) redirect(307, anmeldenAdresse(url));
	return LEER;
}

/**
 * Kopfleiste, Fassung 3: die Projekte für die Seitenleiste (geschützt). Live
 * über denselben Schlüssel wie die Startseite (`api:projects`, Ereignisse
 * `projekt` und `inhalt`). Geht die Liste nicht (Server startet neu), steht
 * die Leiste ohne Projekte da; die Seite selbst lädt trotzdem. Antwortet sie
 * mit 401 oder „fremdes Konto“ (etwa nach dem Abmelden), gilt das als
 * Anmeldestand der Seite, und es geht zur Anmeldung.
 *
 * `projektListe`: die ganze Antwort, damit die Startseite `/api/projects`
 * nicht ein zweites Mal holt (`await parent()` in `+page.ts`).
 */
export const load: LayoutLoad = async ({ fetch, depends, url }): Promise<Daten> => {
	depends('api:projects');
	if (zugang.gesperrt) return zurAnmeldung(url);
	try {
		const l = await api.projects(fetch);
		return { leisteProjekte: l.projects, projektListe: l };
	} catch (e) {
		if (e instanceof ApiError && (e.status === 401 || e.art === 'fremdes-konto')) {
			zugang.setzen(e.status === 401 ? 'fehlt' : 'fremd');
			return zurAnmeldung(url);
		}
		return LEER;
	}
};
