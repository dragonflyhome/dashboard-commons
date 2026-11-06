import { alsLadeFehler, api, ApiError } from '#lib/api/client.ts';
import type { PcVerlaufAntwort } from '$vertrag';
import type { PageLoad } from './$types';

const leer: PcVerlaufAntwort = { werte: [] };

export const load: PageLoad = async ({ fetch, depends }) => {
	depends('api:daten');
	try {
		const bericht = await api.daten(fetch);
		// Ohne Datenspeicher antwortet der Verlauf mit 503; dann gar nicht erst fragen.
		if (bericht.fehler) return { bericht, verlauf: leer };
		try {
			return { bericht, verlauf: await api.pcVerlauf(24, fetch) };
		} catch (e) {
			if (e instanceof ApiError && e.status === 503) return { bericht, verlauf: leer };
			throw e;
		}
	} catch (e) {
		alsLadeFehler(e);
	}
};
