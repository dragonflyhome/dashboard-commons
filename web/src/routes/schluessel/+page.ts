import { alsLadeFehler, api } from '#lib/api/client.ts';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, depends }) => {
	depends('api:schluessel');
	try {
		return await api.keys(fetch);
	} catch (e) {
		alsLadeFehler(e);
	}
};
