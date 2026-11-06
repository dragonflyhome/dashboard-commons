import { alsLadeFehler, api } from '#lib/api/client.ts';
import { ladeBenutzer } from '#lib/pc.ts';
import { ladeVerlauf } from '#lib/zeit/laden.ts';
import type { PageLoad } from './$types';

/** So viele Tage zeigt „Was mit dem PC passiert ist“; der ganze Verlauf steht auf /pc. */
const VERLAUF_TAGE = 7;

/**
 * Projekte, Rechner-Werte und Benutzer-Daten (für Schlüssel- und
 * Ordner-Kachel im Cockpit), dazu der Verlauf der letzten Tage. Ohne
 * Windows-Anmeldung ist `ladung.benutzer` `null`, das Cockpit zeigt dann den Hinweis;
 * der Verlauf kommt dann vom freien Weg (`ohneAnmeldung`, siehe `ladeVerlauf`).
 *
 * Die Projektliste kommt aus dem Layout (`projektListe`, für die
 * Seitenleiste ohnehin geholt): `/api/projects` nur einmal je Laden. Ging
 * sie dort nicht, holt die Seite sie selbst (und zeigt sonst die Fehlerseite).
 */
export const load: PageLoad = async ({ fetch, depends, parent }) => {
	// Live-Lücken: eigene Schlüssel für Benutzer-Daten und Verlauf (Ereignisse
	// pc-benutzer und speicher), statt nur über api:pc mitzuladen.
	depends('api:projects', 'api:pc', 'api:pc:benutzer', 'api:verlauf');
	try {
		const [list, pc, ladung, verlauf] = await Promise.all([
			parent().then((p) => p.projektListe ?? api.projects(fetch)),
			api.pc(fetch),
			ladeBenutzer(fetch),
			ladeVerlauf(fetch, VERLAUF_TAGE)
		]);
		return { ...list, pc, ladung, verlauf };
	} catch (e) {
		alsLadeFehler(e);
	}
};
