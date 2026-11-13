/**
 * Vertragstest: eingefrorene Beispielantworten des Rust-Servers, getippt mit
 * den erzeugten Typen aus `$vertrag`. Der eigentliche Test ist die
 * Typzuweisung (svelte-check meldet jede Abweichung); zur Laufzeit prüfen wir
 * den JSON-Rundlauf und den Zugriff auf Pflichtfelder.
 */
import { describe, expect, it } from 'vitest';
import type {
	Block,
	DatenBericht,
	ProjectDetail,
	ProjectList,
	ProjectSummary,
	PcBenutzer,
	PcReport,
	VorgangsUebersicht
} from '$vertrag';

function rundlauf<T>(wert: T): T {
	return JSON.parse(JSON.stringify(wert)) as T;
}

// ----- Projekte -----

const kopf = {
	slug: 'beispiel',
	title: 'Beispiel',
	status: 'aktiv',
	last_change: '2026-10-25',
	summary: 'Ein Projekt mit Repo.',
	chat: 'Beispiel-Chat',
	backup: 'rot',
	backup_note: 'Repo liegt nur auf C:',
	vorschau: null,
	order: 40,
	open: ['Autostart?'],
	repos: [
		{
			path: 'C:\\beispiel',
			label: 'beispiel',
			private: false,
			branch: 'main',
			commit_hash: 'f05ab8b',
			commit_date: '2026-10-25',
			commit_subject: 'JSON-API und Live-Kanal',
			dirty_files: 0,
			error: null,
			stand: '2026-10-25T18:20:00+01:00'
		}
	],
	file_modified: '2026-10-25T18:20:00+01:00'
} as const;

const erstes: ProjectSummary = {
	...kopf,
	repos: [...kopf.repos],
	open: [...kopf.open],
	bedeutung: 'laeuft',
	backup_bedeutung: 'problem'
};

const neu: ProjectSummary = {
	...erstes,
	slug: 'neues-beispiel',
	title: 'Beispiel mit Daten',
	status: 'geplant',
	bedeutung: 'neutral',
	backup: null,
	backup_bedeutung: null,
	repos: []
};

const liste: ProjectList = {
	projects: [erstes, neu],
	broken: [{ slug: 'kaputt', error: 'Kopf nicht lesbar' }],
	gruppen: []
};

const detail: ProjectDetail = {
	...erstes,
	links: [{ name: 'Repo', url: 'file:///C:/beispiel' }],
	checks: [
		{ name: 'Dashboard', url: 'http://127.0.0.1:8787/healthz', ok: true, status: 200, error: null }
	],
	processes: [{ name: 'dashboard.exe', running: true }],
	seiten: [],
	daten: null
};

/** Die Anzeige-Bausteine, wie `#lib/daten.ts` sie aus `daten/*.toml` macht. */
const bausteine: Block[] = [
	{
		kind: 'gantt',
		dates: true,
		min: 739_921,
		max: 739_930,
		bars: [
			{
				name: 'Phase 0',
				start: 739_921,
				end: 739_930,
				start_iso: '2026-11-01',
				end_iso: '2026-11-10',
				bedeutung: 'ok'
			}
		],
		ticks: [{ at: 739_921, label: '01.11.' }]
	},
	{ kind: 'flow', nodes: [{ id: 'A', label: 'A', layer: 0, slot: 0 }], edges: [] },
	{
		kind: 'progress',
		rows: [{ label: 'Text', percent: 60, text: '3 von 5', note: '', stufe: 'ab-haelfte' }]
	},
	{ kind: 'error', message: 'kaputt' }
];

const detailNeu: ProjectDetail = {
	...neu,
	links: [],
	checks: [],
	processes: [],
	seiten: [{ name: 'labor', title: 'Labor', order: 1 }],
	daten: {
		kennzahlen: {
			teil: {
				plan: {
					kennzahl: [{ titel: 'Preis', wert: '12', einheit: '€', hinweis: null, farbe: 'info' }]
				}
			}
		},
		ampel: { zeile: [{ titel: 'Saatgut', farbe: 'wartet', hinweis: null }] },
		fortschritt: null,
		zeitstrahl: { eintrag: [{ datum: '2026-11', titel: 'Start', text: null, farbe: null }] },
		gantt: {
			start: '2026-11',
			balken: [{ name: 'Phase 0', von: 0, bis: '2027-01', farbe: 'laeuft', gruppe: null }]
		},
		kanban: {
			spalte: [
				{ titel: 'offen', farbe: null, karte: [{ titel: 'Logo', hinweis: null, farbe: null }] }
			]
		},
		fluss: {
			kasten: [
				{ id: 'a', text: 'Aussaat', farbe: null },
				{ id: 'b', text: 'Erntefest', farbe: 'ok' }
			],
			pfeil: [{ von: 'a', nach: 'b', text: null }]
		},
		fragen: {
			frage: [
				{
					id: 'name',
					text: 'Welcher Name?',
					art: 'ja-nein',
					optionen: [],
					empfehlung: null,
					gestellt: '2026-10-26'
				}
			]
		},
		orte: { ort: [{ kennung: 'bauplan', titel: 'Bauplan', hinweis: null }] },
		fristen: null,
		plan: null,
		eigen: { rezept: { menge: 3 } }
	}
};

function beschreibe(b: Block): string {
	switch (b.kind) {
		case 'progress':
			return `progress ${b.rows.length}`;
		case 'ampel':
			return `ampel ${b.rows.length}`;
		case 'kpi':
			return `kpi ${b.items.length}`;
		case 'kanban':
			return `kanban ${b.columns.length}`;
		case 'gantt':
			return `gantt ${b.bars.length}`;
		case 'flow':
			return `flow ${b.nodes.length}`;
		case 'error':
			return `error ${b.message}`;
	}
}

describe('Vertrag: Projekte', () => {
	it('ProjectList mit zwei Projekten', () => {
		const l = rundlauf(liste);
		expect(l).toEqual(liste);
		expect(l.projects.map((p) => [p.bedeutung, p.backup_bedeutung])).toEqual([
			['laeuft', 'problem'],
			['neutral', null]
		]);
		// Etappe 6d: kein Feld form mehr, es gibt nur eine Form.
		expect(l.projects.every((p) => !('form' in p))).toBe(true);
		expect(l.broken[0]?.error).toBe('Kopf nicht lesbar');
	});
	it('ProjectDetail ohne Daten', () => {
		const d = rundlauf(detail);
		expect(d).toEqual(detail);
		expect(d.daten).toBeNull();
	});
	it('Anzeige-Bausteine nach kind', () => {
		const arten = rundlauf(bausteine).map(beschreibe);
		expect(arten).toEqual(['gantt 1', 'flow 1', 'progress 1', 'error kaputt']);
	});
	it('ProjectDetail neue Form: daten, Teile, eigen, seiten', () => {
		const d = rundlauf(detailNeu);
		expect(d).toEqual(detailNeu);
		expect(d.seiten[0]?.title).toBe('Labor');
		const k = d.daten?.kennzahlen;
		expect(k && 'teil' in k ? Object.keys(k.teil) : []).toEqual(['plan']);
		expect(d.daten?.eigen).toEqual({ rezept: { menge: 3 } });
	});
});

// ----- PC -----

const pc: PcReport = {
	snapshot: {
		taken_at: '2026-10-26T08:00:00+01:00',
		host: 'PC',
		os: 'Windows 11 Pro',
		cpu_percent: 12.5,
		cpu_count: 8,
		mem_used: 8_000_000_000,
		mem_total: 16_000_000_000,
		disks: [{ name: 'C:', mount: 'C:\\', total: 1_000, used: 400, belegung: 'normal' }],
		uptime_secs: 3600,
		cpu_belegung: 'normal',
		mem_belegung: 'normal',
		belegung_schwellen: { hoch_ab: 70, kritisch_ab: 90 },
		neustart: { neustart: 'aussteht', gruende: ['windows-update'] },
		defender: {
			checked_at: null,
			antivirus_enabled: true,
			realtime_enabled: true,
			signature_version: null,
			signature_updated: null,
			signature_age_days: 1,
			last_quick_scan: null,
			error: null,
			befund: 'in-ordnung',
			gruende: [],
			signatur_schwelle_tage: 3
		},
		history: [{ at: '2026-10-26T08:00:00+01:00', cpu: 12, mem: 50 }]
	},
	sysapps: {
		refreshed_at: null,
		windows: { product: 'Windows 11 Pro', version: '24H2', build: '26200', installed_at: null },
		services_running: [],
		services_total: 0,
		errors: []
	},
	data: {
		backup: {
			copies: 1,
			external_disk: false,
			cloud: '',
			cloud_stand: null,
			note: '',
			plan: []
		},
		history: [],
		steps: []
	},
	data_error: null,
	backup_stand: null,
	project_backups: [
		{
			slug: 'beispiel',
			title: 'Beispiel',
			chat: null,
			backup: 'rot',
			bedeutung: 'problem',
			backup_note: ''
		}
	]
};

const pcBenutzer: PcBenutzer = {
	inventory: {
		root: 'C:\\',
		computed_at: null,
		duration_secs: 0,
		entries: [],
		face: [],
		hinweis: null
	},
	sysapps: {
		autostart: [{ name: 'Bote', command: 'bote.exe', source: 'HKCU Run' }],
		tools: [{ name: 'git', version: '2.47' }],
		winget: [],
		winget_error: null
	},
	schluessel: { vollstaendig: 'ja', gespeichert: 2, fehlen: 0, grund: null },
	bote: 'bote-fehlt',
	stand: '2026-10-27T08:00:00+01:00'
};

describe('Vertrag: PC', () => {
	it('PcReport-Kopf und Projekt-Backups', () => {
		const r = rundlauf(pc);
		expect(r).toEqual(pc);
		expect(r.snapshot.host).toBe('PC');
		expect(r.snapshot.neustart).toEqual({ neustart: 'aussteht', gruende: ['windows-update'] });
		expect(r.snapshot.defender.befund).toBe('in-ordnung');
		expect(r.project_backups[0]?.bedeutung).toBe('problem');
		expect('keys' in r).toBe(false);
	});
	it('PcBenutzer mit Schlüsseln und Boten-Stand', () => {
		const b = rundlauf(pcBenutzer);
		expect(b).toEqual(pcBenutzer);
		expect(b.schluessel.gespeichert).toBe(2);
		expect(b.bote).toBe('bote-fehlt');
		expect(b.sysapps.tools[0]?.version).toBe('2.47');
	});
});

// ----- Datenspeicher -----

const daten: DatenBericht = {
	pfad: '%USERPROFILE%\\dashboard\\data\\dashboard.redb',
	groesse_bytes: 98_304,
	schema: 1,
	letzte_sicherung: '2026-10-25',
	sicherung_klasse: 'ok',
	sicherungs_ordner: '%USERPROFILE%\\dashboard\\data\\sicherung',
	tabellen: [{ name: 'pc_verlauf', beschreibung: 'PC-Messwerte alle 5 Minuten', eintraege: 288 }],
	pc_verlauf: { erster: '2026-10-24T20:35:00Z', letzter: '2026-10-25T20:35:00Z', eintraege: 288 },
	fehler: null
};

describe('Vertrag: Datenspeicher', () => {
	it('DatenBericht', () => {
		const d = rundlauf(daten);
		expect(d).toEqual(daten);
		expect(d.tabellen.map((t) => t.name)).toEqual(['pc_verlauf']);
		expect(['ok', 'warn', 'bad']).toContain(d.sicherung_klasse);
		expect(d.fehler).toBeNull();
	});
});

// ----- Vorgänge -----

const uebersicht: VorgangsUebersicht = {
	zaehler: { offen: 1, zugestellt: 1, uebernommen_heute: 0, erledigt_heute: 2 },
	chats: [
		{
			slug: 'beispiel',
			wartet_seit: '2026-10-26T08:00:00+01:00',
			zuletzt_uebernommen: null,
			offen: 1
		}
	],
	vorgaenge: [
		{
			id: 'v-20261026-0007',
			art: 'antwort',
			slug: 'beispiel',
			titel: 'Der Inhaber hat geantwortet',
			bezug: 'antwort:beispiel/frage',
			zustand: 'offen',
			erstellt: '2026-10-26T09:14:00+01:00',
			geaendert: '2026-10-26T09:14:00+01:00',
			letzte_nr: 1042,
			von: 'browser',
			notiz: null
		}
	],
	merge_offen: [
		{
			id: 'v-20261030-0003',
			art: 'merge',
			slug: 'dashboard',
			titel: 'Modul architektur: Stufen',
			bezug: null,
			zustand: 'zugestellt',
			erstellt: '2026-10-30T09:14:00+01:00',
			geaendert: '2026-10-30T09:15:00+01:00',
			letzte_nr: 1041,
			von: 'chat:architektur',
			notiz: null,
			merge: {
				zweig: 'projekt/architektur-stufen',
				commit: '0123abc',
				projekt: 'architektur',
				beschreibung: 'GET /api/projekte/architektur/x/stufen',
				pruefbericht: 'cargo test: grün'
			}
		}
	],
	cursor: 1042
};

describe('Vertrag: Vorgänge', () => {
	it('VorgangsUebersicht', () => {
		const u = rundlauf(uebersicht);
		expect(u).toEqual(uebersicht);
		expect(u.vorgaenge.map((v) => v.zustand)).toEqual(['offen']);
		expect(u.zaehler.erledigt_heute).toBe(2);
		expect(u.cursor).toBe(1042);
		expect(u.merge_offen[0]?.merge?.zweig).toBe('projekt/architektur-stufen');
	});
});
