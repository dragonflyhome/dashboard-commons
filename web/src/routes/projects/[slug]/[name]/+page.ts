import { error } from '@sveltejs/kit';
import { alsLadeFehler, api } from '#lib/api/client.ts';
import type { ProjektAntwort } from '$vertrag';
import type { PageLoad } from './$types';

/**
 * Unterseite eines Projekts unter /projects/<slug>/<name>: nur für eine
 * Unterseite aus `seiten` in projekt.toml, sonst 404, wie der Server
 * (`oberflaeche::ist_unterseite`).
 */
export const load: PageLoad = async ({
	fetch,
	params,
	depends
}): Promise<{ kopf: ProjektAntwort }> => {
	depends(`api:projekt:${params.slug}`);
	let kopf: ProjektAntwort;
	try {
		kopf = await api.projekt(params.slug, fetch);
	} catch (e) {
		alsLadeFehler(e);
	}
	if (!kopf.kopf.seiten.some((s) => s.name === params.name))
		error(404, 'Diese Seite gibt es nicht');
	return { kopf };
};
