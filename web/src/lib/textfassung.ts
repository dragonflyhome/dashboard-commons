/**
 * Etappe 3b, Textfassung der Bild-Bausteine (Zielbild, Katalog: „jedes
 * Bild-Baustein hat eine Textfassung“): Was ein Baustein als Bild zeigt
 * (Balken, Ring, Kurve, Streifen, Pfeile), steht hier als Tabelle mit
 * denselben Zahlen und Daten. Die Tabelle zeigt `Textfassung.svelte`: für
 * Vorlese-Programme immer, sichtbar über den Knopf „Als Text“.
 *
 * Jede Funktion bekommt dieselbe Eingabe wie ihr Bild und rechnet mit
 * denselben Hilfen (Datum, Dauer, Zählen je Tag), damit beide Fassungen
 * gleich bleiben. `BILD_BAUSTEINE` ist die Liste aller Bild-Bausteine;
 * textfassung.svelte.spec.ts verlangt, dass jede Datei mit einem Bild darin
 * steht (oder in `KEINE_BILDER` mit Grund) und dass Bild und Text dieselben
 * Zahlen zeigen.
 */
import type {
	Belegung,
	FlowData,
	GanttData,
	KpiItem,
	PcMesswert,
	ProgressRow,
	VerlaufEreignis,
	VerlaufQuelle
} from '$vertrag';
import type { Spalte, Zeile } from './katalog/grund/typen.ts';
import { wort } from './bedeutung.ts';
import type { FortschrittBalken } from './daten.ts';
import { EPOCH_DAYS, fmtDuration } from './timescale.ts';
import { datumAusZahl } from './zeit/achse.ts';
import { DICHTE_TAGE, tageZaehlen } from './zeit/dichte.ts';
import { tagTitel } from './zeit/tage.ts';
import { filtern } from './zeit/verlauf.ts';

/** Eine Textfassung: Name der Tabelle, Spalten und Zeilen. */
export interface Textfassung {
	titel: string;
	spalten: readonly Spalte[];
	zeilen: readonly Zeile[];
	/** Satz, wenn es keine Zeilen gibt. */
	leer: string;
}

/** Wo die Textfassung eines Bild-Bausteins steht. */
export type Ort =
	/** Der Baustein zeigt sie selbst (`<Textfassung …>` in seiner Datei). */
	| 'selbst'
	/**
	 * Wer den Baustein einbindet, zeigt sie gleich darunter. Für die
	 * Zeitachse (Gantt, Dichtestreifen), damit Aufbau und Bedienung der
	 * Bausteine unberührt bleiben.
	 */
	| 'aufrufer'
	/** Der Baustein ist schon Text (Liste, Tabelle) mit allen Zahlen; `grund` sagt, wo. */
	| 'eingebaut';

export interface BildBaustein {
	/** Name im Katalog. */
	name: string;
	ort: Ort;
	/** Name der Funktion hier, die die Textfassung baut (nicht bei `eingebaut`). */
	funktion?: keyof typeof FASSUNGEN;
	grund?: string;
}

// ----- Die Fassungen -----

/** Balkenplan: je Balken Name, von, bis, Dauer und Stand. */
export function ganttText(data: GanttData): Textfassung {
	const einheit = (v: number): number => (data.dates ? v - EPOCH_DAYS : v);
	const zeit = (v: number): string =>
		data.dates ? datumAusZahl(einheit(v)) : `Monat ${String(v)}`;
	return {
		titel: 'Balkenplan als Tabelle',
		spalten: [
			{ schluessel: 'name', titel: 'Name' },
			{ schluessel: 'von', titel: 'Von' },
			{ schluessel: 'bis', titel: 'Bis' },
			{ schluessel: 'dauer', titel: 'Dauer' },
			{ schluessel: 'stand', titel: 'Stand' }
		],
		zeilen: data.bars.map((b) => ({
			name: b.name,
			von: zeit(b.start),
			bis: zeit(b.end),
			dauer: fmtDuration(b.end - b.start, data.dates),
			stand: wort(b.bedeutung)
		})),
		leer: 'Der Plan hat keine Balken.'
	};
}

/**
 * Dichtestreifen: je Tag mit Einträgen die Zahl, dazu die wichtigen (alles
 * außer Routine), wie die Säulen. Tage ohne Eintrag fehlen (im Bild: keine Säule).
 */
export function dichteText(
	ereignisse: readonly VerlaufEreignis[],
	heute: string,
	aus: ReadonlySet<VerlaufQuelle> = new Set(),
	tage = DICHTE_TAGE
): Textfassung {
	const balken = tageZaehlen(filtern(ereignisse, aus), heute, tage).filter((b) => b.anzahl > 0);
	return {
		titel: `Einträge je Tag, letzte ${String(tage)} Tage (nur Tage mit Einträgen)`,
		spalten: [
			{ schluessel: 'tag', titel: 'Tag' },
			{ schluessel: 'anzahl', titel: 'Einträge', art: 'zahl' },
			{ schluessel: 'wichtig', titel: 'davon wichtig', art: 'zahl' }
		],
		// Neuester Tag zuerst, wie im Verlauf darunter.
		zeilen: [...balken]
			.reverse()
			.map((b) => ({ tag: tagTitel(b.tag, heute), anzahl: b.anzahl, wichtig: b.wichtig })),
		leer: `In den letzten ${String(tage)} Tagen gibt es keine Einträge.`
	};
}

/** Ring (Prozessor, Speicher, Platte): Anteil, Wert darunter und Stufe des Servers. */
export function ringText(r: {
	label: string;
	percent: number;
	value: string;
	belegung: Belegung;
	detail?: string | undefined;
}): Textfassung {
	return {
		titel: `${r.label} als Text`,
		spalten: [
			{ schluessel: 'was', titel: 'Was' },
			{ schluessel: 'anteil', titel: 'Anteil', art: 'zahl' },
			{ schluessel: 'wert', titel: 'Wert' },
			{ schluessel: 'stufe', titel: 'Stufe' }
		],
		zeilen: [
			{
				was: r.label,
				anteil: `${String(Math.round(r.percent))} %`,
				wert: r.detail ? `${r.value}, ${r.detail}` : r.value,
				stufe: r.belegung
			}
		],
		leer: ''
	};
}

/** Kleine Kurve (letzte Minuten): jetzt, Spitze, Tiefst, Mittel und Zahl der Messpunkte. */
export function sparkText(values: readonly number[], label: string): Textfassung {
	const n = values.length;
	const runden = (x: number): string => `${String(Math.round(x))} %`;
	const zeilen: Zeile[] =
		n === 0
			? []
			: [
					{ was: 'jetzt', wert: runden(values[n - 1] ?? 0) },
					{ was: 'Spitze', wert: runden(Math.max(...values)) },
					{ was: 'Tiefst', wert: runden(Math.min(...values)) },
					{ was: 'Mittel', wert: runden(values.reduce((a, b) => a + b, 0) / n) },
					{ was: 'Messpunkte', wert: n }
				];
	return {
		titel: `${label} als Text`,
		spalten: [
			{ schluessel: 'was', titel: 'Was' },
			{ schluessel: 'wert', titel: 'Wert', art: 'zahl' }
		],
		zeilen,
		leer: 'Noch keine Messpunkte.'
	};
}

/** Fortschrittsbalken: je Balken Name, Prozent, Stand als Text und Hinweis. */
export function fortschrittText(rows: readonly (ProgressRow | FortschrittBalken)[]): Textfassung {
	return {
		titel: 'Fortschritt als Tabelle',
		spalten: [
			{ schluessel: 'name', titel: 'Name' },
			{ schluessel: 'prozent', titel: 'Prozent', art: 'zahl' },
			{ schluessel: 'stand', titel: 'Stand' },
			{ schluessel: 'hinweis', titel: 'Hinweis' }
		],
		zeilen: rows.map((r) => ({
			name: r.label,
			prozent: `${String(Math.round(r.percent))} %`,
			stand: r.text,
			hinweis: r.note
		})),
		leer: 'Keine Balken.'
	};
}

/** Kennzahlen: je Kachel Bezeichnung, Wert (der Endwert, ohne Hochzählen) und Hinweis. */
export function kennzahlenText(items: readonly KpiItem[]): Textfassung {
	return {
		titel: 'Kennzahlen als Tabelle',
		spalten: [
			{ schluessel: 'name', titel: 'Kennzahl' },
			{ schluessel: 'wert', titel: 'Wert', art: 'zahl' },
			{ schluessel: 'hinweis', titel: 'Hinweis' }
		],
		zeilen: items.map((k) => ({ name: k.label, wert: k.value, hinweis: k.note })),
		leer: 'Keine Kennzahlen.'
	};
}

/** Fluss: je Kasten, wohin seine Pfeile führen. */
export function flussText(data: FlowData): Textfassung {
	return {
		titel: 'Flussdiagramm als Tabelle',
		spalten: [
			{ schluessel: 'kasten', titel: 'Kasten' },
			{ schluessel: 'weiter', titel: 'Führt zu' }
		],
		zeilen: data.nodes.map((n, i) => {
			const ziele = data.edges
				.filter(([a]) => a === i)
				.map(([, b]) => data.nodes[b]?.label ?? '')
				.filter((l) => l !== '');
			return { kasten: n.label, weiter: ziele.length === 0 ? '–' : ziele.join(', ') };
		}),
		leer: 'Das Diagramm hat keine Kästen.'
	};
}

/** Kurve des PC-Verlaufs (/daten): je Reihe Mittel, Spitze, Tiefst und Zahl der Messwerte. */
export function pcKurveText(werte: readonly PcMesswert[]): Textfassung {
	const reihe = (name: string, zahlen: number[]): Zeile => {
		const n = zahlen.length;
		const r = (x: number): string => `${String(Math.round(x))} %`;
		return n === 0
			? { reihe: name, mittel: '–', spitze: '–', tiefst: '–', anzahl: 0 }
			: {
					reihe: name,
					mittel: r(zahlen.reduce((a, b) => a + b, 0) / n),
					spitze: r(Math.max(...zahlen)),
					tiefst: r(Math.min(...zahlen)),
					anzahl: n
				};
	};
	return {
		titel: 'PC-Verlauf als Tabelle',
		spalten: [
			{ schluessel: 'reihe', titel: 'Reihe' },
			{ schluessel: 'mittel', titel: 'Mittel', art: 'zahl' },
			{ schluessel: 'spitze', titel: 'Spitze', art: 'zahl' },
			{ schluessel: 'tiefst', titel: 'Tiefst', art: 'zahl' },
			{ schluessel: 'anzahl', titel: 'Messwerte', art: 'zahl' }
		],
		zeilen:
			werte.length === 0
				? []
				: [
						reihe(
							'Prozessor',
							werte.flatMap((m) => (m.cpu === null ? [] : [m.cpu]))
						),
						reihe(
							'Arbeitsspeicher',
							werte.map((m) => m.mem)
						)
					],
		leer: 'Noch keine Messwerte.'
	};
}

/** Alle Fassungen nach Namen (für die Liste unten und den Test). */
export const FASSUNGEN = {
	ganttText,
	dichteText,
	ringText,
	sparkText,
	fortschrittText,
	kennzahlenText,
	flussText,
	pcKurveText
} as const;

// ----- Die Liste -----

/**
 * Jede Datei unter web/src/lib/katalog und web/src/lib/components, die ein
 * Bild zeichnet (SVG, role=img, role=progressbar, Breite oder Höhe aus Daten),
 * steht hier oder in `KEINE_BILDER`. Pfad relativ zu web/src, mit `/`.
 */
export const BILD_BAUSTEINE: Readonly<Record<string, BildBaustein>> = {
	'lib/katalog/zeit/Gantt.svelte': { name: 'Gantt', ort: 'aufrufer', funktion: 'ganttText' },
	'lib/katalog/zeit/Dichtestreifen.svelte': {
		name: 'Dichtestreifen',
		ort: 'aufrufer',
		funktion: 'dichteText'
	},
	'lib/katalog/zeit/Verlauf.svelte': {
		name: 'Zeitstrahl (Verlauf)',
		ort: 'eingebaut',
		grund:
			'Liste (ul/li) mit Datum, Titel und Art je Ereignis; Linie und Punkte schmücken nur, Zahlen stehen als Text'
	},
	'lib/components/pc/Ring.svelte': { name: 'Ring', ort: 'selbst', funktion: 'ringText' },
	'lib/components/pc/Sparkline.svelte': { name: 'Kurve', ort: 'selbst', funktion: 'sparkText' },
	'lib/components/blocks/Progress.svelte': {
		name: 'Fortschritt',
		ort: 'selbst',
		funktion: 'fortschrittText'
	},
	'lib/components/blocks/Kpi.svelte': {
		name: 'Kennzahlen',
		ort: 'selbst',
		funktion: 'kennzahlenText'
	},
	'lib/components/blocks/Flow.svelte': { name: 'Fluss', ort: 'selbst', funktion: 'flussText' },
	'lib/components/daten/Verlauf.svelte': {
		name: 'PC-Kurve (/daten)',
		ort: 'selbst',
		funktion: 'pcKurveText'
	},
	'lib/components/pc/Folders.svelte': {
		name: 'Ordner (/pc)',
		ort: 'eingebaut',
		grund: 'Unter dem Anteils-Balken steht die Tabelle mit Name, Größe und Dateien je Ordner'
	},
	'lib/katalog/Plan.svelte': {
		name: 'Plan (Projektseite)',
		ort: 'eingebaut',
		grund:
			'Zeitstreifen und kleine Balken sind aria-hidden; die Liste der Etappen darunter nennt Stand-Wort, Datum und „x von y Arbeiten fertig“'
	}
};

/** Dateien, die zwar etwas zeichnen, aber kein Bild aus Daten sind (mit Grund). */
export const KEINE_BILDER: Readonly<Record<string, string>> = {
	'lib/components/StatusDot.svelte': 'Punkt in der Farbe einer Bedeutung, mit Wort als Name',
	'lib/components/pc/BackupCard.svelte': 'Lampe mit Satz daneben, drei Kopien als Text',
	'lib/components/Confetti.svelte': 'Schmuck beim Erledigen, ohne Inhalt',
	'lib/components/Schloss.svelte':
		'Zeichen eines Schlosses, offen oder zu; der Zustand steht im Namen des Links',
	// Kopfleiste, Fassung 3
	'lib/components/kopf/Glocke.svelte':
		'Zeichen einer Glocke; die Zahl steht im Namen des Knopfs, die Meldungen als Liste',
	'lib/katalog/grund/Schublade.svelte': 'Zeichen ☰ und ×; der Knopf hat einen Namen',
	'lib/katalog/Minilinie.svelte':
		'Kleines Symbol der letzten Minute; den Wert nennt der Link (Name und Titel), der ganze Verlauf mit Textfassung steht auf /pc'
};
