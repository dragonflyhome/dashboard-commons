/**
 * Der Reiter „Hallo“: ein Begrüßungstext aus dem privaten Inhalt. Er steht
 * wörtlich in `daten/eigen-hallo.toml` eines Projekts (Handeingabe, Feld
 * `text`). Welches Projekt das ist, steht nicht im Code: `/api/hallo` nennt
 * das erste Projekt mit dieser Datei (oder keins). Der Text kommt dann über
 * `/api/projekte/<projekt>/daten/eigen-hallo`.
 */

/** Wer den Text trägt (`HalloOrt` aus dem Vertrag). */
export const HALLO_ORT = '/api/hallo';
export const HALLO_DATEN = 'eigen-hallo';
/** Die Textfassung zum Herunterladen (Asset, `text/plain; charset=utf-8`). */
export function halloDownload(projekt: string): string {
	return `/projects/${encodeURIComponent(projekt)}/assets/hallo.txt`;
}

/** Inhalt von `eigen-hallo.toml`. */
export interface HalloDaten {
	titel?: string;
	untertitel?: string;
	datum?: string;
	sprache?: string;
	quelle?: string;
	download?: string;
	text?: string;
}

/** Die Felder aus der Antwort der API, nur Text; alles andere fällt weg. */
export function alsHallo(inhalt: unknown): HalloDaten {
	const aus: HalloDaten = {};
	if (typeof inhalt !== 'object' || inhalt === null) return aus;
	const o = inhalt as Record<string, unknown>;
	for (const feld of [
		'titel',
		'untertitel',
		'datum',
		'sprache',
		'quelle',
		'download',
		'text'
	] as const) {
		const wert = o[feld];
		if (typeof wert === 'string') aus[feld] = wert;
	}
	return aus;
}

/** Ein Stück des Texts: ein Absatz (Zeilenumbrüche bleiben) oder eine Liste. */
export type HalloBlock =
	{ art: 'absatz'; text: string } | { art: 'liste'; nummeriert: boolean; punkte: string[] };

const NUMMER = /^\s*\d+[.)]\s+/;
const STRICH = /^\s*[-•*]\s+/;

/**
 * Zerlegt den Text: Leerzeilen trennen Absätze; Zeilen, die mit „1.“ (auch
 * mit Tab davor) oder einem Strich beginnen, werden zu Listen. Text und
 * Emojis bleiben, wie sie sind.
 */
export function zerlegen(text: string): HalloBlock[] {
	const bloecke: HalloBlock[] = [];
	let absatz: string[] = [];
	// Mit `as`, damit TypeScript nicht auf `null` festlegt: die Hilfen unten
	// setzen die Liste in einer Schließung zurück.
	let liste = null as { nummeriert: boolean; punkte: string[] } | null;

	const absatzSchliessen = () => {
		const t = absatz.join('\n').trim();
		if (t) bloecke.push({ art: 'absatz', text: t });
		absatz = [];
	};
	const listeSchliessen = () => {
		if (liste && liste.punkte.length > 0) bloecke.push({ art: 'liste', ...liste });
		liste = null;
	};

	for (const roh of text.replace(/\r\n?/g, '\n').split('\n')) {
		const zeile = roh.replace(/\s+$/, '');
		if (zeile.trim() === '') {
			absatzSchliessen();
			listeSchliessen();
			continue;
		}
		const nummer = NUMMER.exec(zeile);
		const strich = nummer ? null : STRICH.exec(zeile);
		const treffer = nummer ?? strich;
		if (treffer) {
			absatzSchliessen();
			const nummeriert = nummer !== null;
			if (liste?.nummeriert !== nummeriert) {
				listeSchliessen();
				liste = { nummeriert, punkte: [] };
			}
			liste.punkte.push(zeile.slice(treffer[0].length).trim());
			continue;
		}
		if (liste !== null && /^\s/.test(roh)) {
			// Eingerückte Fortsetzung eines Listenpunkts.
			const letzter = liste.punkte.length - 1;
			liste.punkte[letzter] = `${liste.punkte[letzter] ?? ''} ${zeile.trim()}`;
			continue;
		}
		listeSchliessen();
		absatz.push(zeile);
	}
	absatzSchliessen();
	listeSchliessen();
	return bloecke;
}
