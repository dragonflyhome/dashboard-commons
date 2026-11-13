/**
 * Erfundene Beispiel-Einträge für die Tests von „Anstehend“ und dem
 * Unter-Dashboard (kein echter Inhalt, keine Links).
 */
import type { StandAnsicht } from './anstehend.ts';
import type { AnstehendEintrag, AnstehendListe, ProjektStand, Stufe } from '$vertrag';

/** Reihenfolge der Beispiel-Schwerpunkte (wie in data/schwerpunkte.toml). */
const REIHENFOLGE: Record<string, number> = { 'Chor und Verein': 1, 'Bienen und Honig': 5 };

export function eintrag(
	teil: Partial<Omit<AnstehendEintrag, 'stufe'>> & { stufe?: Stufe }
): AnstehendEintrag {
	const { stufe = 'bald', ...rest } = teil;
	const art = rest.art ?? 'aufgabe';
	const id = rest.id ?? 'a-20261029-0001';
	const projekt = rest.projekt ?? 'dashboard';
	const schwerpunkt = rest.schwerpunkt ?? 'Bienen und Honig';
	const e: AnstehendEintrag = {
		art,
		id,
		projekt,
		projekt_titel: 'Dashboard',
		schwerpunkt,
		schwerpunkt_reihenfolge: REIHENFOLGE[schwerpunkt] ?? 99,
		wartet_auf_dich: false,
		titel: 'Beispiel',
		stufe: { vorschlag: stufe, inhaber: null, wirksam: stufe, grund: 'vorschlag' },
		frist: null,
		tage_bis: null,
		frist_stufe: null,
		zustand: art === 'arbeit' ? 'laeuft' : 'offen',
		wartet_auf: art === 'arbeit' ? null : 'inhaber',
		verweis: null,
		vermerk: null,
		ziel: `/projects/${projekt}#${art}-${id}`,
		ziel_da: true,
		quelle: 'datenspeicher:aufgaben',
		hat_link: false,
		knopf: null,
		gestellt: null,
		frage: art === 'frage' ? { art: 'text', optionen: [], empfehlung: null } : null,
		meldung: null,
		von: 'chat:dashboard',
		erstellt: '2026-10-29T08:00:00+01:00',
		...rest
	};
	// Wie die Regel des Servers, wenn der Test sie nicht selbst setzt.
	if (rest.wartet_auf_dich === undefined) {
		e.wartet_auf_dich =
			e.art === 'aufgabe' ||
			e.art === 'frage' ||
			(e.art === 'arbeit' && e.wartet_auf === 'inhaber' && e.vermerk === null);
	}
	return e;
}

/** Vier Einträge über zwei Schwerpunkte und drei Stufen, so wie der Server sie schickt (ohne Zahlen). */
export function beispielListe(): AnstehendListe {
	const eintraege = [
		eintrag({
			id: 'a-1',
			titel: 'Beim Probe-Dienst anmelden',
			stufe: 'jetzt',
			frist: '2026-11-02',
			tage_bis: 4,
			frist_stufe: 'bald',
			hat_link: true
		}),
		eintrag({
			art: 'frage',
			id: 'schlusssatz',
			projekt: 'beispiel',
			projekt_titel: 'Beispiel',
			schwerpunkt: 'Chor und Verein',
			titel: 'Welcher Schlusssatz passt?',
			stufe: 'jetzt',
			frist: '2026-10-30',
			tage_bis: 1,
			frist_stufe: 'bald',
			gestellt: '2026-10-25',
			quelle: 'projects/beispiel/daten/fragen.toml',
			von: null
		}),
		eintrag({
			art: 'arbeit',
			id: 'ar-1',
			titel: 'Seite umbauen',
			stufe: 'irgendwann',
			quelle: 'datenspeicher:arbeiten'
		}),
		eintrag({
			art: 'arbeit',
			id: 'ar-2',
			projekt: 'beispiel',
			projekt_titel: 'Beispiel',
			schwerpunkt: 'Chor und Verein',
			titel: 'Packliste kürzen',
			stufe: 'bald',
			zustand: 'wartet',
			wartet_auf: 'inhaber',
			quelle: 'datenspeicher:arbeiten'
		})
	];
	return {
		heute: '2026-10-29',
		frist_bald_tage: 7,
		eintraege,
		hinweise: [],
		aktualisierungen: []
	};
}

const leereGruppe = { eintraege: [], weitere: 0 };

/** Das Unter-Dashboard, wie die Widgets es bekommen (`standAnsicht`). */
export function beispielStand(teil: Partial<StandAnsicht> = {}): StandAnsicht {
	return {
		slug: 'dashboard',
		schwerpunkt: 'Bienen und Honig',
		gesamt: {
			titel: 'Dashboard',
			status: 'aktiv',
			bedeutung: 'laeuft',
			ampel: 'gruen',
			grund: 'Nichts wartet auf dich.',
			grund_art: 'nichts-wartet',
			bezug_art: null,
			bezug: null,
			zuletzt: '2026-10-29'
		},
		kurz: {
			wartet_auf_dich: 0,
			laeuft: 0,
			als_naechstes: 0,
			naechste_frist: null,
			tage_bis: null,
			frist_stufe: null,
			frist_bald_tage: 7
		},
		frage: { aktuell: null, weitere: 0 },
		aufgaben: { offen: [], anzahl: 0, weitere: 0, sichtbar: false },
		aktualisierungen: [],
		meldungen: { neu: 0, gesamt: 0 },
		arbeiten: { laufen: leereGruppe, wartet: leereGruppe, als_naechstes: leereGruppe },
		notizen: [],
		notizen_zahl: teil.notizen?.length ?? 0,
		fertig_7_tage: 0,
		speicher_fehlt: false,
		fristen: [],
		...teil
	};
}

/** `/stand` oder `/stand/voll`, wie der Server sie schickt (Etappe 3a+). */
export function beispielProjektStand(teil: Partial<ProjektStand> = {}): ProjektStand {
	return {
		slug: 'dashboard',
		schwerpunkt: 'Bienen und Honig',
		gesamt: {
			titel: 'Dashboard',
			status: 'aktiv',
			bedeutung: 'laeuft',
			ampel: 'gruen',
			grund: 'nichts-wartet',
			bezug_art: null,
			bezug: null,
			zuletzt: '2026-10-29'
		},
		frist_bald_tage: 7,
		sichtbar: true,
		eintraege: [],
		aktualisierungen: [],
		zahlen: null,
		notizen: [],
		speicher_fehlt: false,
		...teil
	};
}

/** Die Zahlen des freien Wegs, alle 0. */
export function beispielZahlen(
	teil: Partial<NonNullable<ProjektStand['zahlen']>> = {}
): NonNullable<ProjektStand['zahlen']> {
	return {
		wartet_auf_dich: 0,
		aufgaben: 0,
		fragen: 0,
		laeuft: 0,
		wartet: 0,
		als_naechstes: 0,
		fertig_7_tage: 0,
		meldungen: { neu: 0, gesamt: 0 },
		naechste_frist: null,
		tage_bis: null,
		frist_stufe: null,
		...teil
	};
}

/** Antwort als JSON für ein gestubbtes `fetch`. */
export function json(wert: unknown, status = 200): Response {
	return new Response(JSON.stringify(wert), {
		status,
		headers: { 'content-type': 'application/json' }
	});
}
