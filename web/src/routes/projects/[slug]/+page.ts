import { alsLadeFehler, api } from '#lib/api/client.ts';
import type { PageLoad } from './$types';

/**
 * Die Projektseite: Seite.svelte im Rahmen des Projekts; dafür kommt der
 * Kopf aus projekt.toml dazu (`kopf`).
 */
export const load: PageLoad = async ({ fetch, params, depends }) => {
	depends(`api:project:${params.slug}`);
	depends(`api:projekt:${params.slug}`);
	try {
		const [projekt, kopf] = await Promise.all([
			api.project(params.slug, fetch),
			api.projekt(params.slug, fetch)
		]);
		return { ...projekt, kopf };
	} catch (e) {
		alsLadeFehler(e);
	}
};
