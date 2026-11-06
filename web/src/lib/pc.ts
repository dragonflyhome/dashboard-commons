/**
 * Farbe und Satz für die PC-Übersicht. Die Regeln (3-2-1, Defender, Neustart,
 * Schlüssel, Belegung) rechnet der Server (pcdata.rs, system.rs, api.rs);
 * hier wird nur zugeordnet (Grenze: zustand-vom-server).
 */
import type {
	BackupLage,
	Belegung,
	DefenderGrund,
	DefenderStatus,
	Bedeutung,
	KanbanColumn,
	KopieOrt,
	KopieStand,
	NeustartGrund,
	NeustartLage,
	PcBenutzer,
	SchluesselGrund,
	Sample,
	SchluesselStand,
	Step
} from '$vertrag';
import { AnmeldungNoetig, api, ApiError } from './api/client.ts';
import { SATZ_FREMDES_KONTO, SATZ_NUR_ANGEMELDET } from './anmeldung.ts';
import { fmtDateTime } from './format.ts';
import { mitZugang } from './zugang.ts';

/**
 * Stand der Benutzer-Daten (alles, was der Bote unter dem Konto des Inhabers liest):
 * `da`, `bote-fehlt` (letzter bekannter Stand), `anmeldung-noetig` (ohne
 * Windows-Anmeldung liefert der Server sie nicht aus) oder `fremdes-konto`.
 */
export type Benutzerdaten = PcBenutzer['bote'] | 'anmeldung-noetig' | 'fremdes-konto';

/** Warum `/api/pc/benutzer` nichts geliefert hat. */
export type BenutzerGrund = 'anmeldung' | 'fremdes-konto';

/** Ergebnis von `ladeBenutzer`: die Daten, oder `null` mit Grund. */
export type BenutzerLadung =
	{ benutzer: PcBenutzer; grund: null } | { benutzer: null; grund: BenutzerGrund };

/**
 * Holt `/api/pc/benutzer` (geschützt). Fehlt die Windows-Anmeldung (401) oder
 * ist es ein fremdes Konto (403), kommt `null` mit Grund statt eines Fehlers:
 * Die Rechner-Werte der PC-Seite gehen auch ohne. Andere Fehler fliegen weiter.
 */
export async function ladeBenutzer(f?: typeof fetch): Promise<BenutzerLadung> {
	try {
		// Etappe 3b: ohne mögliche Anmeldung (zugang.ts) ohne Anfrage gleich 401.
		return { benutzer: await api.pcBenutzer(mitZugang(f ?? ((...a) => fetch(...a)))), grund: null };
	} catch (e) {
		if (e instanceof AnmeldungNoetig) return { benutzer: null, grund: 'anmeldung' };
		if (e instanceof ApiError && e.art === 'fremdes-konto') {
			return { benutzer: null, grund: 'fremdes-konto' };
		}
		throw e;
	}
}

/** Stand der Benutzer-Daten aus der Ladung; ein unbekannter `bote`-Wert gilt als da. */
export function benutzerdaten(l: BenutzerLadung): Benutzerdaten {
	if (l.benutzer === null)
		return l.grund === 'fremdes-konto' ? 'fremdes-konto' : 'anmeldung-noetig';
	return l.benutzer.bote === 'bote-fehlt' ? 'bote-fehlt' : 'da';
}

/**
 * Hinweis über einem Benutzer-Teil der PC-Seite, oder `null`, wenn die
 * Daten da sind. `letzterStand`: Zeitpunkt der zuletzt gelesenen Werte.
 */
export function benutzerHinweis(stand: Benutzerdaten, letzterStand: string | null): string | null {
	if (stand === 'anmeldung-noetig') return SATZ_NUR_ANGEMELDET;
	if (stand === 'fremdes-konto') return SATZ_FREMDES_KONTO;
	if (stand === 'bote-fehlt') {
		return letzterStand
			? `Der Bote läuft nicht; letzter Stand ${fmtDateTime(letzterStand)}.`
			: 'Der Bote läuft nicht; einen letzten Stand gibt es noch nicht.';
	}
	return null;
}

export interface Verdict {
	bedeutung: Bedeutung;
	text: string;
}

/** Wie eine Kopie auf der PC-Seite heißt und aussieht, je Ort. */
export const KOPIE_ORTE: readonly { ort: KopieOrt; icon: string; titel: string }[] = [
	{ ort: 'pc', icon: '💻', titel: 'Kopie 1: dieser PC' },
	{ ort: 'zweites-medium', icon: '💾', titel: 'Kopie 2: externe Platte' },
	{ ort: 'anderer-ort', icon: '☁️', titel: 'Kopie 3: anderer Ort' }
];

export interface KopieAnzeige {
	ort: KopieOrt;
	icon: string;
	titel: string;
	stand: KopieStand;
	/** Notiz vom Server (etwa der Cloud-Text), sonst leer. */
	text: string;
}

/**
 * Die Kopien in der Reihenfolge von `KOPIE_ORTE`, mit Titel und Icon. Ein Ort,
 * den der Server nicht schickt, gilt als `fehlt`.
 */
export function backupKopien(s: BackupLage): KopieAnzeige[] {
	return KOPIE_ORTE.map(({ ort, icon, titel }) => {
		const k = s.kopien.find((x) => x.ort === ort);
		return { ort, icon, titel, stand: k?.stand ?? 'fehlt', text: k?.text ?? '' };
	});
}

/**
 * Ampel und Satz zur 3-2-1-Regel. Die Regel selbst rechnet der Server
 * (`regel_321`); hier nur Farbe und der Satz des Inhabers dazu:
 *
 * - erfüllt: ok „gut gesichert“
 * - nicht erfüllt: problem „keine Sicherung“
 * - teilweise: wartet, der Satz nennt, was da ist und was fehlt
 *   (Platte fehlt + Cloud teilweise: „eine Teilkopie in der Cloud, Platte fehlt“;
 *   Platte fehlt + Cloud vorhanden: „Kopie in der Cloud, Platte fehlt“;
 *   Platte da + Cloud teilweise: „Platte da, die Cloud nur teilweise“;
 *   Platte da + keine Cloud: „zwei Kopien, aber keine außer Haus“)
 * - `null` (data/pc.toml hat einen Fehler): neutral „Stand unbekannt“
 */
export function backupVerdict(s: BackupLage | null): Verdict {
	if (s === null) return { bedeutung: 'neutral', text: 'Stand unbekannt' };
	if (s.regel_321 === 'erfuellt') return { bedeutung: 'ok', text: 'gut gesichert' };
	if (s.regel_321 === 'nicht-erfuellt') return { bedeutung: 'problem', text: 'keine Sicherung' };
	const stand = (ort: KopieOrt) => s.kopien.find((k) => k.ort === ort)?.stand ?? 'fehlt';
	const platte = stand('zweites-medium') === 'vorhanden';
	const cloud = stand('anderer-ort');
	let text = 'eine Kopie, aber nicht genug';
	if (!platte && cloud === 'teilweise') text = 'eine Teilkopie in der Cloud, Platte fehlt';
	else if (!platte && cloud === 'vorhanden') text = 'Kopie in der Cloud, Platte fehlt';
	else if (platte && cloud === 'teilweise') text = 'Platte da, die Cloud nur teilweise';
	else if (platte && cloud === 'fehlt') text = 'zwei Kopien, aber keine außer Haus';
	return { bedeutung: 'wartet', text };
}

/**
 * Ein Wort aus einer Tabelle; eine Kennung, die die Oberfläche (noch) nicht
 * kennt, bleibt, wie sie kommt, statt zu verschwinden.
 */
function wortFuer(tabelle: Readonly<Record<string, string>>, kennung: string): string {
	return tabelle[kennung] ?? kennung;
}

/** Gründe des Defenders in Worten (Kennungen vom Server, `DefenderGrund`). */
export const DEFENDER_GRUND: Readonly<Record<DefenderGrund, string>> = {
	'virenschutz-aus': 'Virenschutz aus',
	'echtzeit-aus': 'Echtzeitschutz aus',
	'signatur-alt': 'Signaturen veraltet',
	'signatur-alter-unbekannt': 'Alter der Signaturen unbekannt',
	'noch-nicht-gelesen': 'wird gelesen',
	'abfrage-fehlgeschlagen': 'Abfrage gescheitert'
};

/**
 * Defender: die Regel (Virenschutz, Echtzeit, Signaturalter) rechnet der
 * Server (`befund`, `gruende`, Schwelle `signatur_schwelle_tage`); hier nur
 * Farbe und Satz. `unbekannt` ist nie grün, sondern neutral mit Grund.
 */
export function defenderVerdict(d: DefenderStatus): Verdict {
	const gruende = d.gruende.map((g) => {
		const w = wortFuer(DEFENDER_GRUND, g);
		return g === 'signatur-alt' ? `${w} (älter als ${String(d.signatur_schwelle_tage)} Tage)` : w;
	});
	if (d.befund === 'in-ordnung') return { bedeutung: 'ok', text: 'Defender aktuell' };
	if (d.befund === 'ansehen') {
		return {
			bedeutung: 'problem',
			text: gruende.length > 0 ? `Defender: ${gruende.join(', ')}` : 'Defender: bitte ansehen'
		};
	}
	return {
		bedeutung: 'neutral',
		text: gruende.length > 0 ? `Defender: ${gruende.join(', ')}` : 'Defender: Stand unbekannt'
	};
}

/** Gründe für einen ausstehenden Neustart in Worten (`NeustartGrund`). */
export const NEUSTART_GRUND: Readonly<Record<NeustartGrund, string>> = {
	'windows-update': 'Windows-Update',
	systemkomponenten: 'Systemkomponenten',
	'dateien-warten': 'Dateien warten auf Austausch'
};

/** Neustart: ob er aussteht, sagt der Server (`neustart`); hier Farbe und Satz. */
export function rebootVerdict(n: NeustartLage): Verdict {
	if (n.neustart === 'nicht-noetig') return { bedeutung: 'ok', text: 'kein Neustart nötig' };
	if (n.neustart === 'aussteht') {
		const gruende = n.gruende.map((g) => wortFuer(NEUSTART_GRUND, g));
		return {
			bedeutung: 'wartet',
			text: gruende.length > 0 ? `Neustart steht aus (${gruende.join(', ')})` : 'Neustart steht aus'
		};
	}
	return { bedeutung: 'neutral', text: 'Neustart: unbekannt (nicht lesbar)' };
}

/** Warum der Stand der Schlüssel unbekannt ist, als Satz (`SchluesselGrund`). */
export const SCHLUESSEL_GRUND: Readonly<Record<SchluesselGrund, string>> = {
	anmeldung: 'Schlüssel: nur nach Windows-Anmeldung',
	'fremdes-konto': 'Schlüssel: nicht für dieses Windows-Konto',
	'bote-fehlt': 'Schlüssel: Stand unbekannt (Bote läuft nicht)',
	'liste-unlesbar': 'Schlüssel: Stand unbekannt (Liste nicht lesbar)',
	'tresor-fehler': 'Schlüssel: Stand unbekannt (Tresor meldet einen Fehler)'
};

/**
 * Der Stand der Schlüssel aus der Ladung von `/api/pc/benutzer`. Kam nichts
 * (401 oder 403), ist er `unbekannt` mit dem Grund, den der Server als
 * Fehlerart geschickt hat (`anmeldung`, `fremdes-konto`).
 */
export function schluesselAusLadung(l: BenutzerLadung): SchluesselStand {
	if (l.benutzer !== null) return l.benutzer.schluessel;
	return {
		vollstaendig: 'unbekannt',
		gespeichert: 0,
		fehlen: 0,
		grund: l.grund === 'fremdes-konto' ? 'fremdes-konto' : 'anmeldung'
	};
}

/** Schlüssel: ob alle da sind, sagt der Server (`vollstaendig`); hier Farbe und Satz. */
export function keysVerdict(s: SchluesselStand): Verdict {
	if (s.vollstaendig === 'ja') {
		return {
			bedeutung: 'ok',
			text:
				s.gespeichert === 0
					? 'Schlüssel: keiner angefordert'
					: `Schlüssel: alle ${String(s.gespeichert)} gespeichert`
		};
	}
	if (s.vollstaendig === 'nein') {
		return {
			bedeutung: 'problem',
			text: `Schlüssel: ${String(s.gespeichert)} gespeichert, ${String(s.fehlen)} fehl${s.fehlen === 1 ? 't' : 'en'}`
		};
	}
	return {
		bedeutung: 'neutral',
		text: s.grund === null ? 'Schlüssel: Stand unbekannt' : wortFuer(SCHLUESSEL_GRUND, s.grund)
	};
}

/**
 * Farbe einer Belegung (Prozessor, Arbeitsspeicher, Platte). Die Stufe und
 * ihre Schwellen (`belegung_schwellen`) kommen vom Server; `unbekannt` ist neutral.
 */
export function belegungBedeutung(b: Belegung): Bedeutung {
	switch (b) {
		case 'normal':
			return 'ok';
		case 'hoch':
			return 'wartet';
		case 'kritisch':
			return 'problem';
		case 'unbekannt':
			return 'neutral';
		default:
			// Ein Wert, den die Oberfläche (noch) nicht kennt: nie grün.
			return 'neutral';
	}
}

/** Fahrplan-Schritte als Kanban-Spalten, Reihenfolge wie auf der alten PC-Seite. */
export function stepsToColumns(steps: Step[]): KanbanColumn[] {
	const order: [string, string, KanbanColumn['bedeutung']][] = [
		['wartet', 'wartet auf dich', 'wartet'],
		['offen', 'offen', 'wartet'],
		['vertagt', 'vertagt', 'neutral'],
		['fertig', 'fertig', 'ok']
	];
	const cols: KanbanColumn[] = order.map(([, title, bedeutung]) => ({
		title,
		bedeutung,
		items: []
	}));
	for (const s of steps) {
		const i = order.findIndex(([key]) => key === s.status.toLowerCase());
		const col = cols[i >= 0 ? i : 1];
		if (!col) continue;
		let text = s.when ? `${s.title} (${s.when})` : s.title;
		if (s.note) text += ` · ${s.note}`;
		col.items.push(text);
	}
	return cols.filter((c) => c.items.length > 0);
}

export function percent(part: number, whole: number): number {
	return whole <= 0 ? 0 : Math.max(0, Math.min(100, (part / whole) * 100));
}

/**
 * Die Prozessor-Kurve aus der History des Servers: Messungen ohne Wert
 * (`cpu` null: erste Messung, GetSystemTimes scheiterte, mehrmals genau 0)
 * fehlen, statt als 0 zu erscheinen (Etappe 3a+).
 */
export function cpuVerlauf(history: readonly Pick<Sample, 'cpu'>[]): number[] {
	return history.flatMap((s) => (s.cpu === null ? [] : [s.cpu]));
}
