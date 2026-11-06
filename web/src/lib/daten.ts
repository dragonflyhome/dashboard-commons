/**
 * Neue Form (`projekt.toml` + `daten/*.toml`): die Bausteine aus dem Vertrag
 * in die Props der vorhandenen Block-Komponenten übersetzen.
 *
 * Die Typen `Anzeige`, `Teilansicht` und `Karte` sind Web-eigene Typen (nur
 * für die Oberfläche, nicht im Vertrag mit dem Rust-Server).
 *
 * Nicht angezeigt (Farben wählt die Oberfläche selbst, Text an Pfeilen
 * zeichnet sie nicht; die Prüfung meldet gesetzte Felder als Hinweis):
 * - `Kennzahl.farbe`
 * - `KanbanKarte.farbe`
 * - `FlussKasten.farbe`
 * - `FlussPfeil.text`
 */
import type {
	Ampel,
	AmpelRow,
	Bedeutung,
	Block,
	Datei,
	FlowData,
	FlowNode,
	Fluss,
	Fortschritt,
	FortschrittStufe,
	FortschrittZeile,
	Frage,
	Gantt,
	GanttBar,
	GanttData,
	Kanban,
	KanbanColumn,
	Kennzahlen,
	KpiItem,
	ProgressRow,
	ProjektDaten,
	StandAlter,
	Zeitpunkt
} from '$vertrag';
import { istZahl } from './format.ts';
import { dateToDay, dayToDate } from './timescale.ts';

// ----- Web-eigene Typen -----

/** Was in einer Karte gezeigt wird: ein vorhandener Block oder die Fragenliste. */
export type Anzeige =
	| { art: 'block'; block: Block }
	| { art: 'fragen'; fragen: Frage[] }
	/** Etappe 3a+: Balken mit der Stufe des Servers (oder ohne, dann neutral). */
	| { art: 'fortschritt'; zeilen: FortschrittBalken[] };

/** Ein Teil einer Datei (`[teil.plan]`); `name` ist `null`, wenn die Datei ein Ganzes ist. */
export interface Teilansicht {
	name: string | null;
	anzeige: Anzeige;
}

/** Eine Karte auf der Projektseite, je vorhandenem Baustein. */
export interface Karte {
	schluessel: Exclude<keyof ProjektDaten, 'eigen'>;
	titel: string;
	teile: Teilansicht[];
}

// ----- Datei<T> -----

/**
 * Ganzes oder benannte Teile, in Reihenfolge der Datei: Der Server schickt
 * die Schlüssel von `teil` in dieser Reihenfolge, `Object.entries` behält sie
 * (nur Namen aus lauter Ziffern stellt JavaScript nach vorn).
 */
export function teile<T extends object>(d: Datei<T>): { name: string | null; inhalt: T }[] {
	if ('teil' in d) {
		return Object.entries(d.teil).map(([name, inhalt]) => ({ name, inhalt }));
	}
	return [{ name: null, inhalt: d }];
}

const ohneFarbe: Bedeutung = 'neutral';

// ----- Übersetzungen je Baustein -----

/**
 * Etappe B1: der Stand einer Momentaufnahme („Stand vom …“) mit dem Alter,
 * das der Server gerechnet hat (`stand_alter`).
 */
export interface StandAnzeige {
	/** JJJJ-MM-TT. */
	stand: string;
	alter: StandAlter | null;
}

/** Eine Kachel der Kennzahlen; mit `stand`, wenn die Datei einen nennt. */
export type KennzahlKachel = KpiItem & { stand?: StandAnzeige | undefined };

/** Eine Zeile der Ampel; mit `stand`, wenn die Datei einen nennt. */
export type AmpelZeileAnzeige = AmpelRow & { stand?: StandAnzeige | undefined };

function standVon(z: { stand?: string; stand_alter?: StandAlter }): { stand?: StandAnzeige } {
	return z.stand === undefined ? {} : { stand: { stand: z.stand, alter: z.stand_alter ?? null } };
}

export function kennzahlen(k: Kennzahlen): KennzahlKachel[] {
	return k.kennzahl.map((z) => ({
		label: z.titel,
		value: z.einheit ? `${z.wert} ${z.einheit}` : z.wert,
		note: z.hinweis ?? '',
		...standVon(z)
	}));
}

export function ampel(a: Ampel): AmpelZeileAnzeige[] {
	return a.zeile.map((z) => ({
		bedeutung: z.farbe,
		label: z.titel,
		note: z.hinweis ?? '',
		...standVon(z)
	}));
}

/** Zahl auf Deutsch („1.200“, „0,25“); Prozent mit einer, Mengen mit zwei Nachkommastellen. */
function zahl(n: number, stellen = 1): string {
	if (!istZahl(n)) return '–';
	return n.toLocaleString('de-DE', { maximumFractionDigits: stellen });
}

/**
 * Ein Balken: wie `ProgressRow` des Servers, die Stufe aber nur, wenn der
 * Server sie mitgeschickt hat (`FortschrittZeile.stufe`, Etappe 3a+).
 */
export type FortschrittBalken = Omit<ProgressRow, 'stufe'> & { stufe: FortschrittStufe | null };

/** `prozent` oder `erreicht`/`gesamt` („3 von 5“) → Balken in Prozent (Breite); die Stufe vom Server. */
export function fortschrittZeile(z: FortschrittZeile): FortschrittBalken {
	const { erreicht, gesamt } = z;
	let percent = 0;
	let text = '';
	if (z.prozent !== null) {
		percent = z.prozent;
		text = `${zahl(z.prozent)} %`;
	} else if (erreicht !== null && gesamt !== null) {
		percent = gesamt > 0 ? (erreicht / gesamt) * 100 : 0;
		text = `${zahl(erreicht, 2)} von ${zahl(gesamt, 2)}${z.einheit ? ` ${z.einheit}` : ''}`;
	}
	percent = istZahl(percent) ? Math.min(100, Math.max(0, percent)) : 0;
	return {
		label: z.titel,
		percent,
		text,
		note: z.hinweis ?? '',
		stufe: z.stufe ?? null
	};
}

export function fortschritt(f: Fortschritt): FortschrittBalken[] {
	return f.zeile.map(fortschrittZeile);
}

export function kanban(k: Kanban): KanbanColumn[] {
	return k.spalte.map((s) => ({
		title: s.titel,
		bedeutung: s.farbe ?? ohneFarbe,
		items: s.karte.map((c) => (c.hinweis ? `${c.titel} · ${c.hinweis}` : c.titel))
	}));
}

// ----- Gantt -----

/** „2026-11“ oder „2026-11-03“ → Tage seit 1.1.1 (wie chrono im Rust-Server). */
export function datumZuTag(text: string): number | null {
	const m = /^(\d{4})-(\d{1,2})(?:-(\d{1,2}))?$/.exec(text.trim());
	if (!m) return null;
	const y = Number(m[1]);
	const mo = Number(m[2]);
	const d = m[3] === undefined ? 1 : Number(m[3]);
	if (mo < 1 || mo > 12 || d < 1 || d > 31) return null;
	return dateToDay(new Date(Date.UTC(y, mo - 1, d)));
}

function isoTag(tag: number): string {
	return dayToDate(Math.round(tag)).toISOString().slice(0, 10);
}

/** Ein Plan-Monat hat im Rust-Server 30,4 Tage. */
const TAGE_JE_MONAT = 30.4;

/**
 * Was am Balkenplan nicht stimmt, mit denselben Sätzen wie der Rust-Server
 * (`Gantt::pruefen`); `null`, wenn alles passt. Daten (JJJJ-MM oder
 * JJJJ-MM-TT) werden als Text verglichen, Plan-Monate als Zahl.
 */
export function ganttFehler(g: Gantt): string | null {
	for (const b of g.balken) {
		const { von, bis } = b;
		let vorher: boolean;
		if (typeof von === 'number' && typeof bis === 'number') vorher = bis < von;
		else if (typeof von === 'string' && typeof bis === 'string') vorher = bis < von;
		else {
			return `Beim Balken „${b.name}“ sind \`von\` und \`bis\` verschieden: bitte beide als Datum oder beide als Monatszahl.`;
		}
		if (vorher) return `Beim Balken „${b.name}“ liegt \`bis\` vor \`von\`.`;
	}
	const monat = g.balken.find((b) => typeof b.von === 'number');
	const datum = g.balken.find((b) => typeof b.von === 'string');
	if (monat !== undefined && datum !== undefined) {
		return `Der Balkenplan mischt Plan-Monate und Daten: „${monat.name}“ hat Monatszahlen, „${datum.name}“ hat Daten. Bitte alle Balken gleich, entweder \`von = 0\` oder \`von = "2026-11"\`.`;
	}
	return null;
}

/**
 * Gantt aus `daten/gantt.toml`:
 * Daten werden Tage; Plan-Monate mit `start` werden Tage ab dem Start;
 * Plan-Monate ohne `start` bleiben Monate (Achse „Monat 0, 1, 2 …“).
 * Dieselben Regeln wie `Gantt::pruefen` im Rust-Server: Daten und
 * Plan-Monate nie gemischt (auch nicht mit `start`), `bis` nie vor `von`.
 * Zeilen mit derselben `gruppe` stehen zusammen. Die Gitterlinien zeichnet
 * die Zeitachse selbst; `ticks` bleibt leer.
 */
export function gantt(g: Gantt): GanttData | { fehler: string } {
	if (g.balken.length === 0) return { fehler: 'Im Balkenplan steht noch kein Balken.' };
	const fehler = ganttFehler(g);
	if (fehler !== null) return { fehler };
	const start = g.start === null ? null : datumZuTag(g.start);
	const alleMonate = g.balken.every((b) => typeof b.von === 'number');
	const dates = !alleMonate || start !== null;
	const punkt = (p: Zeitpunkt): number | null => {
		if (typeof p === 'number') return start !== null ? start + p * TAGE_JE_MONAT : p;
		return datumZuTag(p);
	};

	// Gruppen zusammenhalten, in Reihenfolge ihres ersten Auftretens.
	const reihenfolge: (string | null)[] = [];
	for (const b of g.balken) if (!reihenfolge.includes(b.gruppe)) reihenfolge.push(b.gruppe);
	const sortiert = [...g.balken].sort(
		(a, b) => reihenfolge.indexOf(a.gruppe) - reihenfolge.indexOf(b.gruppe)
	);

	const bars: GanttBar[] = [];
	for (const b of sortiert) {
		const a = punkt(b.von);
		const e = punkt(b.bis);
		if (a === null || e === null) {
			return { fehler: `Balken „${b.name}“: Anfang oder Ende ist kein lesbares Datum.` };
		}
		bars.push({
			name: b.name,
			start: a,
			end: e,
			start_iso: dates ? isoTag(a) : null,
			end_iso: dates ? isoTag(e) : null,
			bedeutung: b.farbe ?? ohneFarbe
		});
	}
	const min = Math.min(...bars.map((b) => b.start));
	const max = Math.max(...bars.map((b) => b.end));
	return { dates, min, max: max > min ? max : min + 1, bars, ticks: [] };
}

// ----- Fluss -----

/**
 * Fluss aus `daten/fluss.toml`: Spalte je Kasten = längster Weg von einer
 * Quelle (wie im Rust-Server); Pfeile zu unbekannten Kästen fallen weg.
 */
export function fluss(f: Fluss): FlowData {
	const index = new Map(f.kasten.map((k, i) => [k.id, i] as const));
	const edges: [number, number][] = [];
	for (const p of f.pfeil) {
		const a = index.get(p.von);
		const b = index.get(p.nach);
		if (a === undefined || b === undefined || a === b) continue;
		if (!edges.some(([x, y]) => x === a && y === b)) edges.push([a, b]);
	}
	const n = f.kasten.length;
	const layer = new Array<number>(n).fill(0);
	for (let runde = 0; runde < n; runde++) {
		let geaendert = false;
		for (const [a, b] of edges) {
			const la = layer[a] ?? 0;
			if ((layer[b] ?? 0) < la + 1 && la + 1 < n) {
				layer[b] = la + 1;
				geaendert = true;
			}
		}
		if (!geaendert) break;
	}
	const proSpalte = new Map<number, number>();
	const nodes: FlowNode[] = f.kasten.map((k, i) => {
		const l = layer[i] ?? 0;
		const slot = proSpalte.get(l) ?? 0;
		proSpalte.set(l, slot + 1);
		return { id: k.id, label: k.text, layer: l, slot };
	});
	return { nodes, edges };
}

// ----- alles zusammen -----

function fehlerBlock(message: string): Block {
	return { kind: 'error', message };
}

function karte<T extends object>(
	schluessel: Karte['schluessel'],
	titel: string,
	d: Datei<T> | null,
	zeigen: (inhalt: T) => Anzeige
): Karte | null {
	if (d === null) return null;
	return {
		schluessel,
		titel,
		teile: teile(d).map(({ name, inhalt }) => ({ name, anzeige: zeigen(inhalt) }))
	};
}

/** Je vorhandenem Baustein eine Karte, in fester Reihenfolge. */
export function karten(d: ProjektDaten): Karte[] {
	const block = (b: Block): Anzeige => ({ art: 'block', block: b });
	const alle = [
		karte('kennzahlen', 'Kennzahlen', d.kennzahlen, (k) =>
			block({ kind: 'kpi', items: kennzahlen(k) })
		),
		karte('ampel', 'Ampel', d.ampel, (a) => block({ kind: 'ampel', rows: ampel(a) })),
		karte('fortschritt', 'Fortschritt', d.fortschritt, (f) => ({
			art: 'fortschritt',
			zeilen: fortschritt(f)
		})),
		// Der Zeitstrahl zeigt sich als Verlauf mit den Ereignissen des Servers (`abgeleitet.ereignisse`,
		// über ladeDaten); die Karte trägt darum keine Teile, NeueForm setzt den Baustein selbst ein.
		d.zeitstrahl === null
			? null
			: { schluessel: 'zeitstrahl' as const, titel: 'Zeitstrahl', teile: [] },
		karte('gantt', 'Gantt', d.gantt, (g) => {
			const data = gantt(g);
			return block('fehler' in data ? fehlerBlock(data.fehler) : { kind: 'gantt', ...data });
		}),
		karte('kanban', 'Kanban', d.kanban, (k) => block({ kind: 'kanban', columns: kanban(k) })),
		karte('fluss', 'Fluss', d.fluss, (f) => block({ kind: 'flow', ...fluss(f) })),
		karte('fragen', 'Fragen', d.fragen, (f) => ({ art: 'fragen', fragen: f.frage })),
		// Orte haben den Knopf „Öffnen“ (Projekt aus der Route); NeueForm setzt den Baustein selbst ein.
		d.orte === null
			? null
			: { schluessel: 'orte' as const, titel: 'Orte auf diesem PC', teile: [] },
		// Etappe B3: die Fristenliste rechnet der Server (abgeleitet.fristen über ladeDaten).
		d.fristen === null ? null : { schluessel: 'fristen' as const, titel: 'Fristen', teile: [] }
	];
	return alle.filter((k): k is Karte => k !== null);
}
