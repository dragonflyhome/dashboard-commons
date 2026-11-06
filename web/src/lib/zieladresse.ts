/**
 * Aus dem `ziel` eines Eintrags (vom Server, immer eine Adresse im Dashboard,
 * etwa `/projects/schaufenster#meldung-m-20261030-0001` oder `/pc#meldung-…`)
 * die Adresse für einen Link, gebaut mit `resolve()` und dem Anker. Ein Ziel,
 * das keine bekannte Seite ist, gibt `null` (dann zeigt die Seite den Text
 * ohne Link, nie eine fremde Adresse).
 */
import { resolve } from '$app/paths';

export function zielHref(ziel: string): string | null {
	const i = ziel.indexOf('#');
	const pfad = i >= 0 ? ziel.slice(0, i) : ziel;
	const anker = i >= 0 ? ziel.slice(i) : '';
	const projekt = /^\/projects\/([a-z0-9][a-z0-9-]*)$/.exec(pfad);
	if (projekt?.[1] !== undefined) return resolve('/projects/[slug]', { slug: projekt[1] }) + anker;
	if (pfad === '/pc') return resolve('/pc') + anker;
	if (pfad === '/vorgaenge') return resolve('/vorgaenge') + anker;
	if (pfad === '/anstehend') return resolve('/anstehend') + anker;
	return null;
}

/**
 * Der Slug aus einem Ziel `/projects/<slug>` (ohne Anker), sonst `null`
 * (Fristen im Archiv, Entscheidung archiv-status: Die Seite baut
 * den Link mit `resolve()`).
 */
export function projektAusZiel(ziel: string): string | null {
	return /^\/projects\/([a-z0-9][a-z0-9-]*)$/.exec(ziel)?.[1] ?? null;
}
