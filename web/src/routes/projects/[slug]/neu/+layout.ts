import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import type { LayoutLoad } from './$types';

/**
 * Etappe 6a: /projects/<slug>/neu und /projects/<slug>/neu/<name> sind nur
 * noch Weiterleitungen (Lesezeichen aus der Zeit der Vorschau) auf
 * /projects/<slug> bzw. /projects/<slug>/<name>, dauerhaft und ohne Vorschau.
 * Dieselbe Regel hat der Server (`oberflaeche::weiter_zur_hauptseite`); ob es
 * das Projekt oder die Unterseite gibt, sagt erst das Ziel.
 */
export const load: LayoutLoad = ({ params }) => {
	const slug = params.slug;
	redirect(
		307,
		params.name === undefined
			? resolve('/projects/[slug]', { slug })
			: resolve('/projects/[slug]/[name]', { slug, name: params.name })
	);
};
