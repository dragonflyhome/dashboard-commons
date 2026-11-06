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
