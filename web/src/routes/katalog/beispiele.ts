/**
 * Beispieldaten und Schnipsel für den Katalog (/katalog, Etappe 3a-3).
 * Alles erfunden, keine echten Projekte, keine Links nach außen. Die Formen
 * sind die des Vertrags ($vertrag), also dieselben, die der Server liefert;
 * so zeigt der Katalog die Bausteine mit den Daten, die sie wirklich bekommen.
 */
import type {
	Abgeleitet,
	Aktualisierung,
	Ampel,
	AnstehendEintrag,
	Backlog,
	BacklogKarte,
	Fluss,
	Fortschritt,
	Fragen,
	FristenListe,
	FristZeile,
	Gantt,
	GanttData,
	Kanban,
	Kennzahlen,
	ListeAbgeleitet,
	Orte,
	PlanArbeit,
	PlanEtappe,
	ProjektPlan,
	Schritt,
	VerlaufEreignis
} from '$vertrag';
import type { Spalte } from '$katalog';
import type { StandAnsicht } from '#lib/anstehend.ts';
import { beispielStand, eintrag } from '#lib/anstehend.beispiel.ts';
import { gantt as ganttAusDatei } from '#lib/daten.ts';

/** JJJJ-MM-TT, `tage` von heute aus. */
export function tagIso(tage: number, heute = new Date()): string {
	const d = new Date(heute.getFullYear(), heute.getMonth(), heute.getDate() + tage, 12);
	const zwei = (n: number): string => String(n).padStart(2, '0');
	return `${String(d.getFullYear())}-${zwei(d.getMonth() + 1)}-${zwei(d.getDate())}`;
}

/** Zeitpunkt `tage` von heute aus um `stunde`:00, als RFC 3339 in Ortszeit. */
function zeitIso(tage: number, stunde: number, heute = new Date()): string {
	const d = new Date(heute.getFullYear(), heute.getMonth(), heute.getDate() + tage, stunde, 5);
	return d.toISOString();
}

// ----- Grundteile -----

export const ZEITRAUM = [
	{ wert: '7', text: 'Letzte 7 Tage' },
	{ wert: '30', text: 'Letzte 30 Tage' },
	{ wert: '90', text: 'Letzte 90 Tage' },
	{ wert: 'alles', text: 'Alles (dauert länger)', gesperrt: true }
] as const;

export const ABLAGE_SPALTEN = [
	{ schluessel: 'thema', titel: 'Thema' },
	{ schluessel: 'ordner', titel: 'Ordner', art: 'mono' },
	{ schluessel: 'zeitraum', titel: 'Zeitraum' },
	{ schluessel: 'inhalt', titel: 'Was drin ist' }
] as const;

export const ABLAGE_ZEILEN = [
	{
		thema: 'Steuer',
		ordner: 'D:\\Ablage\\Aemter\\Steuer\\2025',
		zeitraum: '2019 bis heute',
		inhalt: 'Quittungen, Pläne, Briefe vom Gartenverein'
	},
	{
		thema: 'Versicherung',
		ordner: 'D:\\Ablage\\Aemter\\Versicherung',
		zeitraum: 'seit 2021',
		inhalt: 'Verträge, Beitragsrechnungen'
	},
	{
		thema: 'Wohnung',
		ordner: 'D:\\Ablage\\Wohnung\\Mietvertrag',
		zeitraum: '2022',
		inhalt: 'Mietvertrag, Übergabeprotokoll, Nebenkosten'
	}
];

// ----- Daten-Bausteine (Inhalt einer Datei aus daten/) -----

export const KENNZAHLEN: Kennzahlen = {
	kennzahl: [
		{ titel: 'Seiten fertig', wert: '12', einheit: 'von 19', hinweis: null, farbe: 'laeuft' },
		{ titel: 'Offene Fragen', wert: '2', einheit: null, hinweis: 'beide bis Freitag', farbe: null },
		{ titel: 'Bauzeit', wert: '41', einheit: 's', hinweis: 'unter 60 s', farbe: 'ok' },
		// Etappe B1: „Stand vom …“; Alter und Stufe rechnet der Server (hier, was er schicken würde).
		{
			titel: 'Ungelesene Briefe',
			wert: '14',
			einheit: null,
			hinweis: 'von Hand gezählt',
			farbe: null,
			stand: tagIso(-12),
			stand_alter: { tage: 12, stufe: 'alt' }
		}
	]
};

export const AMPEL: Ampel = {
	zeile: [
		{ titel: 'Bau', farbe: 'ok', hinweis: 'zuletzt grün' },
		{
			titel: 'Sicherung',
			farbe: 'wartet',
			hinweis: 'externe Platte fehlt',
			stand: tagIso(-2),
			stand_alter: { tage: 2, stufe: 'frisch' }
		},
		{ titel: 'Prüfung', farbe: 'problem', hinweis: '2 Funde' }
	]
};

export const FORTSCHRITT: Fortschritt = {
	zeile: [
		// `stufe` rechnet der Server beim Lesen (Etappe 3a+); hier steht, was er schicken würde.
		{
			titel: 'Umzug der Projekte',
			prozent: null,
			erreicht: 12,
			gesamt: 19,
			hinweis: null,
			stufe: 'ab-haelfte'
		},
		{
			titel: 'Katalog',
			prozent: 80,
			erreicht: null,
			gesamt: null,
			hinweis: 'Widgets fehlen',
			stufe: 'ab-haelfte'
		},
		// Etappe B1: Kommazahl mit Einheit.
		{
			titel: 'Ablage gesichert',
			prozent: null,
			erreicht: 0.2,
			gesamt: 1.5,
			einheit: 'GB',
			hinweis: null,
			stufe: 'unter-haelfte'
		},
		{
			titel: 'Passkey',
			prozent: 100,
			erreicht: null,
			gesamt: null,
			hinweis: null,
			stufe: 'erreicht'
		}
	]
};

// ----- Tabelle aus Daten (Etappe B1) -----

/** Inhalt einer erfundenen `daten/eigen-kosten.toml` (`[[posten]]`). */
export const KOSTEN = {
	posten: [
		{ name: 'Druckerei', betrag: 120.5, zustand: 'fertig', adresse: '/katalog#marke' },
		{ name: 'Versand', betrag: 18.9, zustand: 'offen' },
		{ name: 'Muster', betrag: 42, zustand: 'fehlt', adresse: '/katalog#stand-hinweis' }
	]
};

/** Was der Server aus `posten` rechnet: Summe der Zahlenspalte, Bedeutung der Statuswörter. */
export const KOSTEN_ABGELEITET: ListeAbgeleitet = {
	anzahl: 3,
	erledigt: { feld: 'zustand', anzahl: 1, anteil: 1 / 3 },
	summe: { betrag: 181.4 },
	datum: {},
	marken: { zustand: ['ok', 'wartet', 'problem'] }
};

export const KOSTEN_SPALTEN: readonly Spalte[] = [
	{ schluessel: 'name', titel: 'Posten', link: 'adresse' },
	{ schluessel: 'zustand', titel: 'Stand', art: 'marke' },
	{ schluessel: 'betrag', titel: 'Betrag in €', art: 'zahl', summe: true }
];

export const KANBAN: Kanban = {
	spalte: [
		{
			titel: 'Als Nächstes',
			farbe: null,
			karte: [{ titel: 'Fristen-Widget', hinweis: null, farbe: null }]
		},
		{
			titel: 'Läuft',
			farbe: 'laeuft',
			karte: [{ titel: 'Katalog', hinweis: 'Etappe 3a-3', farbe: null }]
		},
		{ titel: 'Fertig', farbe: 'ok', karte: [{ titel: 'Token', hinweis: null, farbe: null }] }
	]
};

export const FLUSS: Fluss = {
	kasten: [
		{ id: 'frage', text: 'Frage', farbe: null },
		{ id: 'antwort', text: 'Antwort', farbe: 'laeuft' },
		{ id: 'chat', text: 'Chat geweckt', farbe: 'ok' }
	],
	pfeil: [
		{ von: 'frage', nach: 'antwort', text: 'du' },
		{ von: 'antwort', nach: 'chat', text: null }
	]
};

/** Erfundene Orte; den Pfad bekommt die Oberfläche ohnehin nie. */
export function orte(): Orte {
	return {
		ort: [
			{ kennung: 'bauplan', titel: 'Bauplan (Word)', hinweis: 'Fassung zum Ausdrucken' },
			{ kennung: 'fotos', titel: 'Ordner Fotos', hinweis: null }
		]
	};
}

export function fragen(): Fragen {
	return {
		frage: [
			{
				id: 'farbe',
				text: 'Soll die Ampel im Stand als Rand oben stehen?',
				art: 'ja-nein',
				optionen: [],
				empfehlung: 'ja',
				gestellt: tagIso(-2),
				prioritaet: 'bald'
			},
			{
				id: 'name',
				text: 'Wie soll die Unterseite heißen?',
				art: 'text',
				optionen: [],
				empfehlung: null,
				gestellt: tagIso(-1),
				prioritaet: 'bald'
			}
		]
	};
}

/**
 * Etappe B2: die Warteschlange zu `fragen()` (vom Server im Abgeleiteten von
 * fragen.toml): „farbe“ ist sichtbar, eine weitere wartet.
 */
export function fragenAbgeleitet(): Abgeleitet {
	return {
		heute: tagIso(0),
		listen: {},
		warteschlange: { sichtbar: 'farbe', gestellt: tagIso(-2), weitere: 1 }
	};
}

/** Eine erfundene Karte im Backlog. */
function karte(
	art: BacklogKarte['art'],
	id: string,
	titel: string,
	mehr: Partial<BacklogKarte> = {}
): BacklogKarte {
	return {
		art,
		id,
		titel,
		zustand: 'offen',
		wartet_auf: null,
		frist: null,
		frist_stufe: null,
		hinweis: null,
		ziel: `/projects/beispiel#${art}-${id}`,
		geaendert: `${tagIso(-1)}T09:00:00+01:00`,
		etappe: null,
		besitzer: null,
		schritte: [],
		...mehr
	};
}

/** Ein erfundener Schritt einer Arbeit (Zweig teilschritte). */
function schritt(nr: number, titel: string, stand: Schritt['stand']): Schritt {
	return {
		nr,
		titel,
		stand,
		besitzer: 'chat:beispiel',
		wartet_auf: null,
		verweis: null,
		angeboten_an: null,
		fuer: null,
		seit: `${tagIso(-1)}T10:00:00+01:00`,
		notiz: null,
		vorgang: null
	};
}

/** Eine erfundene Zeile der Fristenliste. */
function fristZeile(
	quelle: FristZeile['quelle'],
	id: string,
	titel: string,
	tage: number | null,
	mehr: Partial<FristZeile> = {}
): FristZeile {
	const frist = tage === null ? null : tagIso(tage);
	const stufe = (t: number): FristZeile['frist_stufe'] =>
		t < 0 ? 'ueberfaellig' : t === 0 ? 'heute' : t <= 7 ? 'bald' : 'spaeter';
	return {
		quelle,
		id,
		projekt: 'beispiel',
		projekt_titel: 'Beispielprojekt',
		titel,
		frist,
		tage_bis: tage,
		frist_stufe: tage === null ? null : stufe(tage),
		frist_bezug: null,
		ereignis: null,
		hinweis: null,
		ziel:
			quelle === 'datei' ? `/projects/beispiel#frist-${id}` : `/projects/beispiel#${quelle}-${id}`,
		...mehr
	};
}

/** Eine erfundene Fristenliste (Etappe B3), so wie `abgeleitet.fristen` sie liefert. */
export function fristenListe(): FristenListe {
	return {
		heute: tagIso(0),
		frist_bald_tage: 7,
		zeilen: [
			fristZeile('datei', 'plakate', 'Plakate bestellen', -1),
			fristZeile('aufgabe', 'a-20261104-0001', 'Getränkeliste prüfen und abschicken', 3),
			fristZeile('datei', 'standanmeldung', 'Stand anmelden', 26, {
				frist_bezug: { ereignis: 'zusage-da', abstand: '+1 Monat' },
				ereignis: { kennung: 'zusage-da', titel: 'Zusage ist da', datum: tagIso(-5) }
			}),
			fristZeile('datei', 'helferliste', 'Helferliste verschicken', null, {
				frist_bezug: { ereignis: 'aufbau', abstand: '-3 Tage' },
				ereignis: { kennung: 'aufbau', titel: 'Termin für den Aufbau', datum: null },
				hinweis: 'Termin steht noch aus'
			})
		],
		hinweise: [],
		speicher_fehlt: false
	};
}

/** Ein erfundenes Backlog (Etappe B2), so wie `/api/projekte/<slug>/backlog` es liefert. */
export function backlog(): Backlog {
	const frage = karte('frage', 'farbe', 'Soll die Ampel im Stand als Rand oben stehen?', {
		wartet_auf: 'inhaber',
		hinweis: 'noch 1 Frage wartet'
	});
	const aufgabe = karte('aufgabe', 'a-20261103-0001', 'Beim Beispielportal anmelden', {
		wartet_auf: 'inhaber',
		frist: tagIso(0),
		frist_stufe: 'heute'
	});
	const extern = karte('arbeit', 'ar-20261102-0002', 'Angebot der Druckerei abwarten', {
		zustand: 'wartet',
		wartet_auf: 'extern'
	});
	const chat = karte('arbeit', 'ar-20261102-0003', 'Texte vom Nachbarprojekt übernehmen', {
		zustand: 'wartet',
		wartet_auf: 'chat:nachbar'
	});
	return {
		slug: 'beispiel',
		heute: tagIso(0),
		spalten: [
			{ art: 'fuer-dich', titel: 'Für dich', bedeutung: 'wartet', karten: [frage, aufgabe] },
			{
				art: 'laeuft',
				titel: 'Läuft',
				bedeutung: 'laeuft',
				karten: [
					karte('arbeit', 'ar-20261102-0001', 'Rundmail entwerfen', {
						zustand: 'laeuft',
						etappe: 'beispiel/einladung',
						besitzer: 'chat:beispiel',
						schritte: [
							schritt(1, 'Entwurf schreiben', 'fertig'),
							schritt(2, 'Verteiler prüfen', 'laeuft')
						]
					})
				]
			},
			{ art: 'wartet', titel: 'Wartet', bedeutung: 'info', karten: [extern, chat] },
			{
				art: 'als-naechstes',
				titel: 'Als Nächstes',
				bedeutung: 'neutral',
				karten: [karte('arbeit', 'ar-20261102-0004', 'Fotos sichten', { zustand: 'als-naechstes' })]
			}
		],
		wartend: [
			{ rolle: 'inhaber', wer: 'inhaber', titel: 'Wartet auf dich', karten: [frage, aufgabe] },
			{
				rolle: 'chat',
				wer: 'chat:nachbar',
				titel: 'Wartet auf den Chat „nachbar“',
				karten: [chat]
			},
			{ rolle: 'extern', wer: 'extern', titel: 'Wartet auf jemanden draußen', karten: [extern] }
		],
		fragen_weitere: 1,
		kanban_datei: false,
		speicher_fehlt: false
	};
}

/** Eine erfundene Etappe, so wie `/plan` sie liefert. */
function etappe(
	kennung: string,
	titel: string,
	reihenfolge: number,
	mehr: Partial<PlanEtappe> = {}
): PlanEtappe {
	return {
		kennung,
		titel,
		ziel: `Ziel von „${titel}“.`,
		reihenfolge,
		ab: null,
		stand: 'geplant',
		arbeiten_fertig: 0,
		arbeiten_gesamt: 0,
		begonnen: null,
		fertig: null,
		ungefaehr: false,
		gewachsen_seit: null,
		dazugekommen: 0,
		arbeiten: [],
		...mehr
	};
}

/** Eine erfundene Arbeit im Plan. */
function planArbeit(id: string, titel: string, mehr: Partial<PlanArbeit> = {}): PlanArbeit {
	return {
		id,
		titel,
		stand: 'als-naechstes',
		aus: null,
		archiviert: false,
		etappe: null,
		fertig: null,
		ungefaehr: false,
		...mehr
	};
}

/** Ein erfundener Plan (Etappe B8), so wie `/api/projekte/<slug>/plan` ihn liefert. */
export function projektPlan(heute = new Date()): ProjektPlan {
	const zeit = (tage: number, uhr: string): string => `${tagIso(tage, heute)}T${uhr}:00+01:00`;
	return {
		projekt: 'beispiel',
		etappen: [
			etappe('vorbereitung', 'Vorbereitung', 1, {
				stand: 'fertig',
				arbeiten_fertig: 2,
				arbeiten_gesamt: 2,
				begonnen: zeit(-20, '09:00'),
				fertig: zeit(-12, '17:00'),
				ungefaehr: true,
				arbeiten: [
					planArbeit('ar-20261020-0001', 'Saal anfragen', {
						stand: 'fertig',
						archiviert: true,
						fertig: zeit(-15, '12:00')
					}),
					planArbeit('ar-20261020-0002', 'Liste der Gäste', {
						stand: 'fertig',
						fertig: zeit(-12, '17:00'),
						ungefaehr: true
					})
				]
			}),
			etappe('einladung', 'Einladung', 2, {
				stand: 'laeuft',
				arbeiten_fertig: 0,
				arbeiten_gesamt: 2,
				begonnen: zeit(-10, '09:00'),
				gewachsen_seit: zeit(-10, '09:00'),
				dazugekommen: 1,
				arbeiten: [
					planArbeit('ar-20261102-0001', 'Rundmail entwerfen', { stand: 'laeuft' }),
					planArbeit('ar-20261102-0007', 'Druck prüfen', { aus: 'druckerei' })
				]
			}),
			etappe('fest', 'Das Fest', 3, { ab: tagIso(40, heute).slice(0, 7) })
		],
		ohne_etappe: [],
		hinweise: []
	};
}

export function gantt(): Gantt {
	return {
		start: null,
		balken: [
			{ name: 'Token', von: tagIso(-20), bis: tagIso(-12), farbe: 'ok', gruppe: null },
			{ name: 'Katalog', von: tagIso(-10), bis: tagIso(2), farbe: 'laeuft', gruppe: null },
			{ name: 'Urteile', von: tagIso(3), bis: tagIso(10), farbe: null, gruppe: null }
		]
	};
}

// ----- Zeit (3a-1) -----

let nr = 0;
function ereignis(
	tage: number,
	stunde: number,
	mehr: Partial<VerlaufEreignis> = {}
): VerlaufEreignis {
	nr += 1;
	return {
		kennung: `katalog:${String(nr)}`,
		zeit: zeitIso(tage, stunde),
		genauigkeit: 'minute',
		bis: null,
		titel: `Eintrag ${String(nr)}`,
		text: null,
		quelle: 'hand',
		art: 'erledigt',
		verweis: null,
		...mehr
	};
}

export function verlauf(): VerlaufEreignis[] {
	nr = 0;
	return [
		ereignis(3, 9, { titel: 'Abnahme 3a-3', art: 'geplant', genauigkeit: 'tag' }),
		ereignis(0, 9, { titel: 'Katalog gebaut' }),
		ereignis(0, 8, { quelle: 'update', art: 'routine', titel: 'Defender-Update' }),
		ereignis(0, 7, { quelle: 'update', art: 'routine', titel: 'Defender-Update' }),
		ereignis(-1, 16, { quelle: 'meldung', art: 'problem', titel: 'Bau rot: Tabelle zu breit' }),
		ereignis(-1, 10, { titel: 'Token zusammengelegt', text: 'st- und c- sind jetzt b-.' }),
		ereignis(-4, 11, { titel: 'Zeitachse fertig', art: 'erledigt' }),
		ereignis(-12, 10, { titel: 'Plan 3a bestätigt', art: 'begonnen' }),
		ereignis(-40, 10, { titel: 'Zielbild freigegeben' })
	];
}

/** Balkenplan der Zeitachse (zeit/Gantt.svelte), aus demselben Plan wie oben. */
export function ganttDaten(): GanttData | { fehler: string } {
	return ganttAusDatei(gantt());
}

// ----- Widgets (A4) -----

export interface WidgetBeispiel {
	stand: StandAnsicht;
	fragen: AnstehendEintrag[];
	aufgaben: AnstehendEintrag[];
	arbeiten: AnstehendEintrag[];
	aktualisierungen: Aktualisierung[];
}

/** Ein Projekt, auf das einiges wartet: so sieht man alle Teile der Widgets. */
export function widgetBeispiel(): WidgetBeispiel {
	const frage1 = eintrag({
		art: 'frage',
		id: 'k-frage-1',
		projekt: 'katalog',
		titel: 'Soll die Ampel als Rand oben stehen?',
		stufe: 'jetzt',
		gestellt: tagIso(-2),
		frage: { art: 'ja-nein', optionen: [], empfehlung: 'ja' }
	});
	const frage2 = eintrag({
		art: 'frage',
		id: 'k-frage-2',
		projekt: 'katalog',
		titel: 'Welche Schrift für Zahlen?',
		gestellt: tagIso(-1)
	});
	const aufgabe = eintrag({
		id: 'k-aufgabe-1',
		projekt: 'katalog',
		titel: 'Probe-Termin bestätigen',
		frist: tagIso(2),
		tage_bis: 2,
		frist_stufe: 'bald'
	});
	const aufgabe2 = eintrag({
		id: 'k-aufgabe-2',
		projekt: 'katalog',
		titel: 'Belege hochladen',
		frist: tagIso(6),
		tage_bis: 6,
		frist_stufe: 'bald'
	});
	const laeuft = eintrag({
		art: 'arbeit',
		id: 'k-arbeit-1',
		projekt: 'katalog',
		titel: 'Katalog bauen',
		zustand: 'laeuft',
		frist: tagIso(0),
		tage_bis: 0,
		frist_stufe: 'heute'
	});
	const wartet = eintrag({
		art: 'arbeit',
		id: 'k-arbeit-2',
		projekt: 'katalog',
		titel: 'Freigabe der Farben',
		zustand: 'wartet',
		wartet_auf: 'inhaber'
	});
	const naechstes = eintrag({
		art: 'arbeit',
		id: 'k-arbeit-3',
		projekt: 'katalog',
		titel: 'Urteile nach der Grenze',
		zustand: 'als-naechstes'
	});
	const stand = beispielStand({
		slug: 'katalog',
		gesamt: {
			titel: 'Katalog',
			status: 'aktiv',
			bedeutung: 'laeuft',
			ampel: 'gelb',
			grund: '4 Einträge warten auf dich.',
			grund_art: 'wartet',
			bezug_art: null,
			bezug: null,
			zuletzt: tagIso(0)
		},
		kurz: {
			wartet_auf_dich: 4,
			laeuft: 1,
			als_naechstes: 1,
			naechste_frist: tagIso(0),
			tage_bis: 0,
			frist_stufe: 'heute',
			frist_bald_tage: 7
		},
		notizen: ['Bilder für den Katalog fehlen noch']
	});
	const meldung = (id: string, titel: string, gelesen: boolean, tage: number): Aktualisierung => ({
		quelle: 'meldung',
		id,
		art: 'fehler',
		titel,
		zeit: zeitIso(tage, 9),
		ziel: `/katalog#meldung-${id}`,
		gelesen,
		anzahl: 1,
		behoben: gelesen,
		text: gelesen ? null : 'Die Tabelle „Wo was liegt“ ist breiter als der Bildschirm.'
	});
	return {
		stand,
		fragen: [frage1, frage2],
		aufgaben: [aufgabe, aufgabe2],
		arbeiten: [laeuft, wartet, naechstes],
		aktualisierungen: [
			meldung('k-m-1', 'Prüfung: Tabelle zu breit', false, 0),
			{
				quelle: 'vorgang',
				id: 'k-v-1',
				art: 'antwort',
				titel: 'Antwort auf „Weiter?“: ja',
				zeit: zeitIso(-1, 15),
				ziel: '/katalog#widgets',
				gelesen: true,
				anzahl: 1,
				behoben: false,
				text: null
			},
			meldung('k-m-2', 'Bau wieder grün', true, -2)
		]
	};
}

// ----- Schnipsel (wie man es benutzt) -----

export const SCHNIPSEL = {
	knopf: `<Knopf art="haupt" onclick={speichern}>Speichern</Knopf>
<Knopf art="leise" onclick={abbrechen}>Abbrechen</Knopf>
<Knopf laeuft={laeuft ? 'wird gespeichert' : null}>Speichern</Knopf>`,
	schalter: `<Schalter bind:an={nurOffene} beschriftung="Nur offene zeigen"
  hinweis="Erledigte bleiben gespeichert." />`,
	auswahl: `<Auswahl beschriftung="Zeitraum" bind:wert={zeitraum}
  optionen={[{ wert: '7', text: 'Letzte 7 Tage' }, { wert: '30', text: 'Letzte 30 Tage' }]} />`,
	dialog: `<Dialog titel="Eintrag löschen?" beschreibung="Das lässt sich nicht zurückholen."
  ausloeser="Löschen…">
  {#snippet children(schliessen)}
    <Knopf art="haupt" onclick={() => { loeschen(); schliessen(); }}>Ja, löschen</Knopf>
  {/snippet}
</Dialog>`,
	kurzinfo: `Fristen <Kurzinfo text="Zählt nur offene Einträge mit Datum" name="Mehr zu Fristen">ⓘ</Kurzinfo>`,
	menue: `<Menue
  beschriftung="Mehr"
  punkte={[
    { text: 'Daten', href: resolve('/daten') },
    { text: 'Hallo', href: resolve('/hallo'), aktiv: true }
  ]}
/>`,
	schublade: `<Schublade beschriftung="Seiten und Projekte" titel="Seiten und Projekte" bind:offen>
  <Seitenleiste … ongewaehlt={() => (offen = false)} />
</Schublade>`,
	klappfeld: `<Klappfeld name="Meldungen: 2 ungelesen" titel="Meldungen">
  {#snippet knopf()}🔔{/snippet}
  {#snippet children(schliessen)}<a href={ziel} onclick={schliessen}>Bau rot</a>{/snippet}
</Klappfeld>`,
	minilinie: `<Minilinie werte={cpuLetzteMinute} plaetze={12} bedeutung="ok" />`,
	reiter: `<Reiter titel={['Plan', 'Stand']}>
  {#snippet children(gewaehlt)}
    {#if gewaehlt === 0}<Gantt daten={plan} />{:else}<Fortschritt daten={stand} />{/if}
  {/snippet}
</Reiter>`,
	aufklapper: `<Aufklapper titel="Ältere Einträge">
  <Liste punkte={alte} />
</Aufklapper>`,
	tabelleDaten: `<!-- Etappe B1: daten/eigen-kosten.toml
[[posten]]
name = "Druckerei"
betrag = 120.5
zustand = "fertig"          # Farb- oder Statuswort: wird eine Marke
adresse = "/katalog#marke"  # https://… oder ein Pfad dieser App
-->
<Tabelle beschriftung="Kosten im Oktober" daten={ladeDaten('eigen-kosten')} liste="posten"
  spalten={[{ schluessel: 'name', titel: 'Posten', link: 'adresse' },
            { schluessel: 'zustand', titel: 'Stand', art: 'marke' },
            { schluessel: 'betrag', titel: 'Betrag in €', art: 'zahl', summe: true }]} />`,
	marke: `<Marke bedeutung="ok">bezahlt</Marke>
<Marke bedeutung="wartet" wort="offen" />
<!-- in einer Tabelle: Spalte mit art: 'marke', die Bedeutung sagt der Server -->`,
	standHinweis: `<!-- daten/kennzahlen.toml oder ampel.toml -->
stand = "2026-10-30"   # Tag der Momentaufnahme
alt_ab = 14            # ab 14 Tagen gelb (ohne Angabe: 7)

<!-- Kennzahlen und Ampel zeigen ihn von selbst; sonst: -->
<StandHinweis stand={k.stand} alter={k.stand_alter} />`,
	tabelle: `<!-- Mit Daten -->
<Tabelle beschriftung="Wo was liegt" mindestbreite={36}
  spalten={[{ schluessel: 'thema', titel: 'Thema' },
            { schluessel: 'ordner', titel: 'Ordner', art: 'mono' }]}
  zeilen={ablage.eintrag} />

<!-- Eigene Zeilen (etwa mit Links in den Zellen) -->
<Tabelle beschriftung="Wo was liegt" mindestbreite={36}>
  <thead><tr><th>Thema</th><th>Ordner</th></tr></thead>
  <tbody>
    {#each ablage.eintrag as e, i (i)}<tr><td>{e.thema}</td><td>{e.ordner}</td></tr>{/each}
  </tbody>
</Tabelle>`,
	abschnitt: `<Abschnitt titel="Zeitplan" unterzeile="Stand: 30.10.">…</Abschnitt>`,
	absatz: `<Absatz>Ein Absatz Fließtext.</Absatz>`,
	text: `<Text text={'Erster Absatz.\\n\\nZweiter Absatz.'} />`,
	liste: `<Liste punkte={['Erstens', 'Zweitens']} nummeriert />`,
	hinweis: `<Hinweis bedeutung="wartet">Die externe Platte fehlt seit drei Tagen.</Hinweis>`,
	bild: `<Bild datei="foto.jpg" alt="Was auf dem Bild zu sehen ist" titel="Überschrift" />`,
	kennzahlen: `const k = ladeDaten<Kennzahlen>(slug, 'kennzahlen');
<Kennzahlen daten={k} />

<!-- daten/kennzahlen.toml (Etappe B1) -->
[[kennzahl]]
titel = "Offene Fragen"     # leere Liste: 0, kein Fehler
berechnet = { quelle = "fragen", liste = "frage", art = "anzahl" }

[[kennzahl]]
titel = "Seiten freigegeben"  # „40 %“: wo freigabe = "freigegeben"
berechnet = { quelle = "eigen-seiten", art = "anteil", feld = "freigabe", wert = "freigegeben" }

[[kennzahl]]
titel = "Ungelesene Briefe"
wert = "14"
stand = "2026-10-30"        # „Stand vom 30.10.2026“, gelb ab 7 Tagen`,
	ampel: `<Ampel daten={ladeDaten<Ampel>(slug, 'ampel')} />`,
	fortschritt: `<Fortschritt daten={ladeDaten<Fortschritt>(slug, 'fortschritt')} />

<!-- daten/fortschritt.toml: auch mit Kommazahl (Etappe B1) -->
[[zeile]]
titel = "Ablage gesichert"
erreicht = 0.2
gesamt = 1.5
einheit = "GB"              # „0,2 von 1,5 GB“`,
	kanban: `<Kanban daten={ladeDaten<Kanban>(slug, 'kanban')} />`,
	fluss: `<Fluss daten={ladeDaten<Fluss>(slug, 'fluss')} />`,
	fragen: `<Fragen daten={ladeDaten<Fragen>(slug, 'fragen')} />
<!-- Zeigt nur die eine sichtbare Frage der Warteschlange (Etappe B2), dahinter
     „noch N Fragen warten“. Reihenfolge: Stufe, dann Frist, dann die Datei. -->`,
	backlog: `<!-- Steht schon oben auf jeder Projektseite (nicht in Seite.svelte einbauen). -->
<Backlog backlog={b} />   <!-- b: GET /api/projekte/<slug>/backlog -->`,
	fristenliste: `<!-- In Seite.svelte; die Liste rechnet der Server aus daten/fristen.toml und den Aufgaben. -->
<Fristenliste daten={ladeDaten('fristen')} />`,
	orte: `<Orte daten={ladeDaten('orte')} />`,
	gantt: `<Gantt daten={ladeDaten<Gantt>(slug, 'gantt')} />`,
	zeitstrahl: `<Zeitstrahl daten={ladeDaten<Zeitstrahl>(slug, 'zeitstrahl')} />`,
	verlauf: `<Verlauf ereignisse={ereignisse} modus="projekt" ebene={3} />`,
	dichtestreifen: `<Dichtestreifen {ereignisse} onwaehle={(tag) => springe(tag)} />`,
	zeitachse: `<div bind:clientWidth={breite}>
  <Zeitachse fenster={{ von: heute - 30, bis: heute + 10 }} {breite} {heute} />
</div>`,
	zeitgantt: `<Gantt data={plan} />  <!-- zeit/Gantt.svelte, Daten aus gantt() in #lib/daten.ts -->
<Textfassung fassung={ganttText(plan)} />`,
	wahl: `<Wahl bind:wert={ansicht} beschriftung="Ansicht"
  optionen={[{ wert: 'kacheln', text: 'Kacheln' }, { wert: 'spalten', text: 'Spalten' }]} />`,
	textGantt: `<Gantt data={plan} />
<Textfassung fassung={ganttText(plan)} />  <!-- #lib/textfassung.ts -->`,
	textDichte: `<Dichtestreifen {ereignisse} aus={ausgeblendet} />
<Textfassung fassung={dichteText(ereignisse, heute, ausgeblendet)} />`,
	textRing: `<Ring label="Arbeitsspeicher" percent={71.6} value="23 GB / 32 GB" belegung="hoch" />
<!-- Textfassung steckt im Ring (nur für Vorlese-Programme) -->`,
	textKurve: `<Sparkline values={werte} label="Prozessor, letzte 15 Minuten" />`,
	textFortschritt: `<Fortschritt daten={ladeDaten<Fortschritt>(slug, 'fortschritt')} />
<!-- Knopf „Als Text“ kommt von selbst mit -->`,
	textKennzahlen: `<Kennzahlen daten={ladeDaten<Kennzahlen>(slug, 'kennzahlen')} />`,
	textFluss: `<Fluss daten={ladeDaten<Fluss>(slug, 'fluss')} />`,
	textPcKurve: `<Verlauf werte={messwerte} />  <!-- components/daten/Verlauf.svelte -->`,
	textEingebaut: `<!-- BILD_BAUSTEINE in #lib/textfassung.ts, ort: 'eingebaut' -->`,
	stand: `<Stand gesamt={stand.gesamt} schwerpunkt={stand.schwerpunkt} kurz={stand.kurz}
  speicherFehlt={stand.speicher_fehlt} />`,
	frage: `<Frage zahl={wartet} {fragen} aufgaben={offeneAufgaben} arbeiten={arbeitenFuerDich}>
  {#snippet antwort(e)}<FrageAntwort eintrag={e} … />{/snippet}
</Frage>`,
	arbeiten: `<Arbeiten gruppen={[{ wort: 'läuft', eintraege, weitere: 0 }, …]} />`,
	fristen: `<Fristen naechste={stand.kurz.naechste_frist} tageBis={stand.kurz.tage_bis}
  fristStufe={stand.kurz.frist_stufe} eintraege={eintraege} />`,
	zuletzt: `<Zuletzt {aktualisierungen} weitere={ungeleseneAusserhalb} ungelesen={neu}
  angemeldet ongelesen={(id, knopf) => gelesen(id, knopf)} bind:titel />`
} as const;
