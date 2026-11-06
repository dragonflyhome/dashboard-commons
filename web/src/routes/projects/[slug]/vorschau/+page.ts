import { error } from '@sveltejs/kit';
import { alsLadeFehler, api } from '#lib/api/client.ts';
import type { PageLoad } from './$types';

/** Nur, was der Rahmen braucht; der Entwurf selbst kommt als Datei aus assets/. */
export interface PreviewData {
	slug: string;
	title: string;
	/** Adresse des Entwurfs auf dem Rust-Server. */
	vorschau: string;
}

export const load: PageLoad = async ({ fetch, params, depends }): Promise<PreviewData> => {
	depends(`api:project:${params.slug}`);
	let vorschau: string | null;
	let title: string;
	try {
		const p = await api.project(params.slug, fetch);
		vorschau = p.vorschau;
		title = p.title;
	} catch (e) {
		alsLadeFehler(e);
	}
	if (vorschau === null) {
		error(
			404,
			`„${title}“ hat keine Vorschau. Der Chat setzt dafür in projekt.toml das Feld vorschau = "assets/….html".`
		);
	}
	return { slug: params.slug, title, vorschau };
};
