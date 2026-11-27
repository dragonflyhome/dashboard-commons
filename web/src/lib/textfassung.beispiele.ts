/**
 * Etappe 3b: Beispiele für die Bild-Bausteine mit Textfassung. Dieselben
 * Daten zeigt /katalog (Abschnitt „Als Text“) und prüft
 * textfassung.svelte.spec.ts (Bild und Text mit denselben Zahlen). Alles
 * erfunden.
 */
import type {
	Belegung,
	FlowData,
	GanttData,
	KalenderJahr,
	KpiItem,
	PcMesswert,
	ProgressRow,
	VerlaufEreignis
} from '$vertrag';
import { gantt } from './daten.ts';

export const GANTT: GanttData = (() => {
	const plan = gantt({
		start: null,
		balken: [
			{ name: 'Token', von: '2026-09-01', bis: '2026-09-20', farbe: 'ok', gruppe: null },
			{ name: 'Katalog', von: '2026-09-15', bis: '2026-10-28', farbe: 'laeuft', gruppe: null },
			{ name: 'Schönheit', von: '2026-10-31', bis: '2026-11-14', farbe: null, gruppe: null }
		]
	});
	if ('fehler' in plan) throw new Error(plan.fehler);
	return plan;
})();

export const FLUSS: FlowData = {
	nodes: [
		{ id: 'frage', label: 'Frage', layer: 0, slot: 0 },
		{ id: 'antwort', label: 'Antwort', layer: 1, slot: 0 },
		{ id: 'chat', label: 'Chat geweckt', layer: 2, slot: 0 },
		{ id: 'notiz', label: 'Notiz', layer: 2, slot: 1 }
	],
	edges: [
		[0, 1],
		[1, 2],
		[1, 3]
	]
};

export const KENNZAHLEN: KpiItem[] = [
	{ label: 'Seiten fertig', value: '12 von 19', note: '' },
	{ label: 'Offene Fragen', value: '2', note: 'beide bis Freitag' },
	{ label: 'Bauzeit', value: '41 s', note: 'unter 60 s' }
];

export const FORTSCHRITT: ProgressRow[] = [
	{ label: 'Umzug der Projekte', percent: 63.2, text: '12 von 19', note: '', stufe: 'ab-haelfte' },
	{ label: 'Katalog', percent: 80, text: '80 %', note: 'Widgets fehlen', stufe: 'ab-haelfte' },
	{ label: 'Logo', percent: 20, text: '1 von 5', note: '', stufe: 'unter-haelfte' }
];

export const RING: {
	label: string;
	percent: number;
	value: string;
	belegung: Belegung;
	detail?: string;
} = { label: 'Arbeitsspeicher', percent: 71.6, value: '23 GB / 32 GB', belegung: 'hoch' };

export const KURVE: { label: string; values: number[] } = {
	label: 'Prozessor, letzte 15 Minuten',
	values: [12, 18, 9, 33, 41, 27, 15, 22]
};

export const PC_KURVE: PcMesswert[] = [
	{ zeit: '2026-10-31T08:00:00Z', cpu: 12, mem: 48, platte_belegt: null, platte_gesamt: null },
	{ zeit: '2026-10-31T08:01:00Z', cpu: null, mem: 50, platte_belegt: null, platte_gesamt: null },
	{ zeit: '2026-10-31T08:02:00Z', cpu: 30, mem: 55, platte_belegt: null, platte_gesamt: null },
	{ zeit: '2026-10-31T08:03:00Z', cpu: 21, mem: 51, platte_belegt: null, platte_gesamt: null }
];

/** Ereignisse für den Dichtestreifen, Tage relativ zu `heute` (Mittag, Ortszeit). */
export function dichteEreignisse(heute: Date): VerlaufEreignis[] {
	const zeit = (tage: number, stunde: number): string =>
		new Date(
			heute.getFullYear(),
			heute.getMonth(),
			heute.getDate() + tage,
			stunde,
			5
		).toISOString();
	let nr = 0;
	const e = (tage: number, stunde: number, art: VerlaufEreignis['art']): VerlaufEreignis => {
		nr += 1;
		return {
			kennung: `textfassung:${String(nr)}`,
			zeit: zeit(tage, stunde),
			genauigkeit: 'minute',
			bis: null,
			titel: `Eintrag ${String(nr)}`,
			text: null,
			quelle: art === 'routine' ? 'update' : 'hand',
			art,
			verweis: null
		};
	};
	return [
		e(0, 9, 'erledigt'),
		e(0, 8, 'routine'),
		e(0, 7, 'routine'),
		e(-1, 16, 'problem'),
		e(-1, 10, 'erledigt'),
		e(-4, 11, 'erledigt'),
		e(-12, 10, 'begonnen'),
		e(-12, 11, 'routine')
	];
}

/**
 * Kalender, Stufe 6: ein Festjahr des Jahreskreises (2026/27, nach dem Beispieljahr; die
 * Uhrzeiten und die Viertel zwischen Neu- und Vollmond sind erfunden).
 */
export const JAHR: KalenderJahr = (() => {
	const monate: [string, string, string, string | null][] = [
		['2026-12-09', '2026-12-24', '2. Wintervollmond · Yule-Mond', 'Wintervollmond'],
		['2027-01-07', '2027-01-22', 'Imbolc', 'Wintervollmond'],
		['2027-02-06', '2027-02-21', '1. Frühlingsvollmond', 'Frühlingsvollmond'],
		['2027-03-08', '2027-03-22', '2. Frühlingsvollmond · Ostara-Mond', 'Frühlingsvollmond'],
		['2027-04-07', '2027-04-21', 'Beltane', 'Frühlingsvollmond'],
		['2027-05-06', '2027-05-20', '1. Sommervollmond', 'Sommervollmond'],
		['2027-06-04', '2027-06-19', '2. Sommervollmond · Litha-Mond', 'Sommervollmond'],
		['2027-07-04', '2027-07-18', 'Lughnassad (Lammas)', 'Sommervollmond'],
		['2027-08-02', '2027-08-17', '1. Herbstvollmond', 'Herbstvollmond'],
		['2027-08-31', '2027-09-16', '2. Herbstvollmond · Mabon-Mond', 'Herbstvollmond'],
		['2027-09-30', '2027-10-15', 'Samhain', 'Herbstvollmond'],
		['2027-10-29', '2027-11-14', '1. Wintervollmond', 'Wintervollmond']
	];
	const ende = '2027-11-28';
	return {
		zeitraum: { anfang: '2026-12-09', ende },
		monate: monate.map(([anfang, vollmond, name, jahreszeit], i) => ({
			nummer: i + 1,
			name,
			namen: [],
			jahreszeit,
			zeitraum: { anfang, vollmond, ende: monate[i + 1]?.[0] ?? ende }
		})),
		sonnentage: [
			{ kennung: 'wende-winter', name: 'Wintersonnenwende', datum: '2026-12-21', zeit: '21:50' },
			{
				kennung: 'gleiche-fruehling',
				name: 'Frühlings-Tagundnachtgleiche',
				datum: '2027-03-20',
				zeit: '21:24'
			},
			{ kennung: 'wende-sommer', name: 'Sommersonnenwende', datum: '2027-06-21', zeit: '16:10' },
			{
				kennung: 'gleiche-herbst',
				name: 'Herbst-Tagundnachtgleiche',
				datum: '2027-09-23',
				zeit: '08:01'
			}
		],
		viertel: monate.flatMap(([anfang, vollmond]) => [
			{ kennung: 'neumond', name: 'Neumond', datum: anfang, zeit: '12:00' },
			{ kennung: 'vollmond', name: 'Vollmond', datum: vollmond, zeit: '18:00' }
		]),
		// Ab dem 21.6.2027, wie der Server sie liefert (drei, auch über das Festjahr hinaus).
		naechste_vollmonde: ['2027-07-18', '2027-08-17', '2027-09-16'].map((datum) => ({
			kennung: 'vollmond',
			name: 'Vollmond',
			datum,
			zeit: '18:00'
		})),
		hinweis: null
	};
})();
