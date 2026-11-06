import { alsLadeFehler, getJson } from '#lib/api/client.ts';
import { HALLO_DATEN, HALLO_ORT, alsHallo, halloDownload } from '#lib/hallo.ts';
import type { DatenAntwort, HalloOrt } from '$vertrag';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, depends }) => {
	// Welches Projekt den Text trägt, sagt der Server (kein Slug im Code).
	try {
		const ort = await getJson<HalloOrt>(HALLO_ORT, fetch);
		if (!ort.projekt) return { hallo: alsHallo(null), download: null };
		// Live-Lücken: nur dieses Projekt (projekt-Ereignis zu hallo, Sammelereignis), nicht jedes.
		depends(`api:projekt:${ort.projekt}`);
		const antwort = await getJson<DatenAntwort>(
			`/api/projekte/${encodeURIComponent(ort.projekt)}/daten/${HALLO_DATEN}`,
			fetch
		);
		return { hallo: alsHallo(antwort.inhalt), download: halloDownload(ort.projekt) };
	} catch (e) {
		alsLadeFehler(e);
	}
};
