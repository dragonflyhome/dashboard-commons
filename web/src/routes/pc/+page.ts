import { alsLadeFehler, api } from '#lib/api/client.ts';
import { ladeBenutzer } from '#lib/pc.ts';
import { ladeVerlauf } from '#lib/zeit/laden.ts';
import type { PageLoad } from './$types';

/**
 * Rechner-Werte (`/api/pc`, frei), Benutzer-Daten (`/api/pc/benutzer`,
 * geschützt) und der Verlauf (`/api/pc/verlauf/voll`, geschützt, Hotfix 3a-1)
 * nebeneinander. Fehlt die Windows-Anmeldung, ist `benutzer` `null` mit
 * `grund: 'anmeldung'`; die Seite zeigt dann den Hinweis statt einer
 * Fehlerseite. Der Verlauf kommt dann vom freien Weg ohne Starts, Meldungen
 * und Vorgänge (`ohneAnmeldung`, mit ruhigem Hinweis).
 */
export const load: PageLoad = async ({ fetch, depends }) => {
	// Live-Lücken: eigene Schlüssel für Benutzer-Daten und Verlauf (Ereignisse
	// pc-benutzer und speicher).
	depends('api:pc', 'api:pc:benutzer', 'api:verlauf');
	try {
		const [pc, ladung, verlauf] = await Promise.all([
			api.pc(fetch),
			ladeBenutzer(fetch),
			ladeVerlauf(fetch)
		]);
		return { pc, ...ladung, verlauf };
	} catch (e) {
		alsLadeFehler(e);
	}
};
