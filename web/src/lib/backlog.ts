/**
 * Das Backlog eines Projekts laden (Etappe B2): `GET /api/projekte/<slug>/backlog`,
 * geschützt wie `/stand/voll`. Gerechnet ist alles im Server (Spalten nach
 * Stand und Status, Fragen-Warteschlange, Wartende je Verantwortlichem);
 * hier wird nur geholt.
 */
import { API } from './api/client';
import type { Backlog } from '$vertrag';

/** Holt das Backlog; `null` ohne Anmeldung, bei Fehlern und ohne Projekt. Wirft nie. */
export async function ladeBacklog(
	slug: string,
	fetchFn: typeof fetch = fetch
): Promise<Backlog | null> {
	try {
		const res = await fetchFn(`${API}/projekte/${encodeURIComponent(slug)}/backlog`, {
			credentials: 'same-origin',
			headers: { accept: 'application/json' }
		});
		return res.ok ? ((await res.json()) as Backlog) : null;
	} catch {
		return null;
	}
}
