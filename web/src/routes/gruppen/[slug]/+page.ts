import { error } from '@sveltejs/kit';
import type { GruppeAnzeige, ProjectSummary } from '$vertrag';
import type { PageLoad } from './$types';

/**
 * Die Gruppenseite (Entscheidung projektgruppe): die Gruppe aus
 * der Projektliste des Layouts (`projektListe.gruppen`, live über
 * `api:projects`). Die Teile laden ihre Seiten selbst (`GruppenTeil`), damit
 * eine Änderung an einem Teil nur diesen Abschnitt erneuert. Eine Gruppe,
 * die es nicht (mehr) gibt, ist 404; ließ sich die Liste nicht laden, 503.
 */
export const load: PageLoad = async ({
	params,
	parent
}): Promise<{ gruppe: GruppeAnzeige; teile: Record<string, ProjectSummary> }> => {
	const { projektListe } = await parent();
	if (projektListe === null)
		error(503, 'Die Projekte lassen sich gerade nicht laden. Gleich noch einmal versuchen.');
	const gruppe = projektListe.gruppen.find((g) => g.slug === params.slug);
	if (gruppe === undefined) error(404, `Die Gruppe „${params.slug}“ gibt es nicht.`);
	const teile: Record<string, ProjectSummary> = {};
	for (const p of projektListe.projects) if (gruppe.teile.includes(p.slug)) teile[p.slug] = p;
	return { gruppe, teile };
};
