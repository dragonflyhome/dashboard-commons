/**
 * Sätze und Wörter der Widgets (Etappe 3a-3, A4). Der Server liefert
 * Fakten (Frist, Tage bis, wartet_auf, Art); Satz und Farbe entstehen hier
 * (Grenze: zustand-vom-server [grenze_bestaetigt]).
 */
import {
	fertigeSchritte,
	fristWorte,
	kurzDatum,
	MELDUNG_WORT,
	mitFristNaechsteZuerst
} from '#lib/anstehend.ts';
import { chatWort } from '#lib/aufgaben.ts';
import type {
	AnstehendEintrag,
	ArbeitsStand,
	Bedeutung,
	FristStufe,
	MeldungsArt,
	Schritt,
	StandAmpel
} from '$vertrag';

/** Ampel des Stands als Bedeutung (Farbe) und Wort (für Vorlese-Programme). */
export const AMPEL: Record<StandAmpel, { bedeutung: Bedeutung; wort: string }> = {
	gruen: { bedeutung: 'ok', wort: 'grün' },
	gelb: { bedeutung: 'wartet', wort: 'gelb' },
	rot: { bedeutung: 'problem', wort: 'rot' },
	unbekannt: { bedeutung: 'neutral', wort: 'unbekannt' }
};

/**
 * Warum ein Eintrag in seiner Stufe steht, als Satz (Etappe 3b, Fund des
 * Prüfers: „Stufe: Bald (Vorschlag des Chats)“ verstand niemand).
 */
export function wannSatz(e: Pick<AnstehendEintrag, 'stufe'>): string {
	const wann = { jetzt: 'jetzt dran', bald: 'bald dran', irgendwann: 'irgendwann dran' }[
		e.stufe.wirksam
	];
	if (e.stufe.grund === 'inhaber') return `Du hast es auf „${wann}“ gesetzt.`;
	if (e.stufe.grund === 'frist') return `Wegen der Frist ${wann}.`;
	if (e.stufe.grund === 'fest') return `Meldungen sind immer ${wann}.`;
	return `Der Chat schlägt vor: ${wann}.`;
}

/** Wen ein Eintrag braucht, als kurzer Satz. */
export function wartetAufWort(w: string | null): string | null {
	if (w === null) return null;
	if (w === 'inhaber') return 'wartet auf dich';
	if (w.startsWith('chat:')) return `wartet auf Chat ${w.slice(5)}`;
	if (w === 'extern') return 'wartet auf jemand anderen';
	if (w.startsWith('schritt:')) return `wartet auf Schritt ${w.slice(8)}`;
	return `wartet auf ${w}`;
}

// ----- Teilschritte (Zweig teilschritte, T4) -----

/** Stand eines Schritts als Wort. */
export const SCHRITT_STAND: Record<ArbeitsStand, string> = {
	laeuft: 'läuft',
	wartet: 'wartet',
	'als-naechstes': 'als Nächstes',
	fertig: 'fertig'
};

/** Farbe eines Schritts, nur zusätzlich zum Wort. */
export const SCHRITT_BEDEUTUNG: Record<ArbeitsStand, Bedeutung> = {
	laeuft: 'laeuft',
	wartet: 'wartet',
	'als-naechstes': 'neutral',
	fertig: 'ok'
};

const angebotSatz = (an: string): string => `angeboten an ${chatWort(an)}, noch nicht übernommen`;

/** Die zugeklappte Zeile: „3 von 7 fertig · dran: Chat architektur“, dazu offene Angebote. */
export function schritteKurz(schritte: readonly Schritt[]): string {
	const teile = [`${String(fertigeSchritte(schritte))} von ${String(schritte.length)} fertig`];
	const dran = new Set(
		schritte.flatMap((s) =>
			s.besitzer !== null && (s.stand === 'laeuft' || s.stand === 'wartet')
				? [chatWort(s.besitzer)]
				: []
		)
	);
	if (dran.size > 0) teile.push(`dran: ${[...dran].join(', ')}`);
	const angebote = new Set(schritte.flatMap((s) => (s.angeboten_an ? [s.angeboten_an] : [])));
	for (const an of angebote) teile.push(angebotSatz(an));
	return teile.join(' · ');
}

/** Stand eines Schritts in Worten; beim Warten worauf („wartet auf Schritt 2“). */
export function schrittStand(s: Schritt): string {
	return s.stand === 'wartet' && s.wartet_auf !== null
		? (wartetAufWort(s.wartet_auf) ?? SCHRITT_STAND.wartet)
		: SCHRITT_STAND[s.stand];
}

/** Wer dran ist, wem er angeboten ist oder dass er frei ist. */
export function schrittWer(s: Schritt): string | null {
	if (s.angeboten_an) return angebotSatz(s.angeboten_an);
	if (s.besitzer === null) return s.stand === 'fertig' ? null : 'noch frei';
	return s.stand === 'fertig'
		? `erledigt von ${chatWort(s.besitzer)}`
		: `dran: ${chatWort(s.besitzer)}`;
}

/**
 * `2026-11-05T14:30:00+01:00` → „seit 05.11., 14:30 Uhr“ (Ortszeit, wie der
 * Server sie schreibt); bei fertigen Schritten „am …“, denn da dauert nichts an.
 */
export function zeitText(s: Pick<Schritt, 'stand' | 'seit'>): string {
	const wann = `${kurzDatum(s.seit.slice(0, 10))}, ${s.seit.slice(11, 16)} Uhr`;
	return s.stand === 'fertig' ? `am ${wann}` : `seit ${wann}`;
}

export const SATZ_STILLSTAND = 'Seit über einem Tag unverändert.';
const TAG_MS = 24 * 60 * 60 * 1000;

/**
 * Stillstand: ein Schritt, der läuft, wartet oder angeboten ist, ohne
 * Änderung seit über 24 Stunden. Freie und fertige stehen nicht still.
 */
export function stillstand(s: Schritt, jetzt: number): boolean {
	if (s.stand === 'fertig' || (s.besitzer === null && !s.angeboten_an)) return false;
	const t = Date.parse(s.seit);
	return !Number.isNaN(t) && jetzt - t > TAG_MS;
}

/**
 * Der Anker der Frage oder Aufgabe, auf die ein Schritt wartet: Aufgaben
 * haben Kennungen `a-JJJJMMTT-NNNN` (ab der 10.000. am Tag mehr Stellen),
 * alles andere ist eine Frage aus fragen.toml.
 */
export function verweisZiel(verweis: string): { anker: string; wort: string } {
	return /^a-\d{8}-\d{4,}$/.test(verweis)
		? { anker: `aufgabe-${verweis}`, wort: 'zur Aufgabe' }
		: { anker: `frage-${verweis}`, wort: 'zur Frage' };
}

/** „bis 31.10., in 2 Tagen“ oder `null` ohne Frist. */
export function fristText(e: Pick<AnstehendEintrag, 'frist' | 'tage_bis'>): string | null {
	if (e.frist === null) return null;
	const w = fristWorte(e.tage_bis);
	return w === null ? `bis ${kurzDatum(e.frist)}` : `bis ${kurzDatum(e.frist)}, ${w}`;
}

/**
 * Farbe einer Frist aus der Frist-Stufe des Servers (Etappe 3a-4): überfällig
 * Problem, heute wartet, sonst keine. Dieselbe Stufe hebt im Server die
 * Stufe des Eintrags; Rand und Reihenfolge kommen so aus einer Quelle.
 */
export function fristBedeutung(stufe: FristStufe | null): Bedeutung | null {
	if (stufe === 'ueberfaellig') return 'problem';
	if (stufe === 'heute') return 'wartet';
	return null;
}

/** Wort einer Vorgangsart in „Zuletzt“. */
export const VORGANG_WORT: Readonly<Record<string, string>> = {
	antwort: 'Antwort',
	bau: 'Seite erneuern',
	abnahme: 'Abnahme',
	wunsch: 'Wunsch',
	frage: 'Frage',
	schritt: 'Teilschritt'
};

/** Wort einer Meldungsart; Unbekanntes bleibt, wie es kommt. */
export function meldungsWort(art: string): string {
	return art in MELDUNG_WORT ? MELDUNG_WORT[art as MeldungsArt] : art;
}

/** Einträge mit Frist, die nächste zuerst; geordnet wird in anstehend.ts (Etappe 3a+). */
export const nachFrist = mitFristNaechsteZuerst;
