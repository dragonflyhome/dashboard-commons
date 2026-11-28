import { fireEvent, render, screen } from '@testing-library/svelte';
import { flushSync, type ComponentProps } from 'svelte';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type {
	Aktualisierung,
	Betrieb,
	DatenBericht,
	PcMesswert,
	ProjectSummary,
	Schritt,
	Vorgang,
	VorgangDetail
} from '$vertrag';
import Verlauf from '../components/daten/Verlauf.svelte';
import ProjectCard from '../components/ProjectCard.svelte';
import DashboardStand from '../components/pc/DashboardStand.svelte';
import VorgangZeile from '../components/vorgaenge/VorgangZeile.svelte';
import Plan from '../katalog/Plan.svelte';
import Meldung from '../katalog/widgets/Meldung.svelte';
import Schritte from '../katalog/widgets/Schritte.svelte';
import ZeitVerlauf from '../katalog/zeit/Verlauf.svelte';
import { bauAnzeige } from '../projekte.ts';
import { projektPlan } from '../../routes/katalog/beispiele.ts';
import { LaufendesJetzt } from './jetzt.svelte.ts';
import { tagInZone, zoneSetzen } from './zone.svelte.ts';

vi.mock('$app/navigation', () => ({ invalidate: vi.fn() }));
vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));

const { default: DatenSeite } = await import('../../routes/daten/+page.svelte');

// Gerät in Berlin (MEZ, +1), Blickort in Auckland (NZDT, +13).
const vorherTz = process.env.TZ;
beforeAll(() => {
	process.env.TZ = 'Europe/Berlin';
});
afterAll(() => {
	if (vorherTz === undefined) delete process.env.TZ;
	else process.env.TZ = vorherTz;
});
afterEach(() => {
	zoneSetzen(null);
	vi.useRealTimers();
});

const AUCKLAND = 'Pacific/Auckland';
/** Berlin Fr 27.11.2026 12:30, Auckland schon Sa 28.11.2026 00:30. */
const JETZT = new Date('2026-11-27T11:30:00Z');

/** Der ganze Text der Seite. */
const text = (): string => document.body.textContent;

function wert(zeit: string, cpu: number, mem: number): PcMesswert {
	return { zeit, cpu, mem, platte_belegt: null, platte_gesamt: null };
}

describe('Zone gesetzt nach dem Zeichnen ergibt neuen Text', () => {
	it('PC-Verlauf auf /daten: Von und Bis in der neuen Zone', () => {
		const { container } = render(Verlauf, {
			werte: [wert('2026-11-27T10:55:00Z', 10, 40), wert('2026-11-27T11:30:00Z', 20, 50)]
		});
		const titel = (): string | null | undefined =>
			container.querySelector('svg title')?.textContent;
		expect(titel()).toMatch(/PC-Verlauf von 11:55 bis 12:30 Uhr/);
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(titel()).toMatch(/PC-Verlauf von 23:55 bis 00:30 Uhr/);
	});

	it('Seite Daten: erster und letzter Messwert in der neuen Zone', () => {
		const bericht: DatenBericht = {
			pfad: 'dashboard.redb',
			groesse_bytes: 0,
			schema: 1,
			letzte_sicherung: null,
			sicherung_klasse: 'bad',
			sicherungs_ordner: 'sicherung',
			tabellen: [],
			pc_verlauf: { erster: '2026-11-26T11:30:00Z', letzter: '2026-11-27T11:30:00Z', eintraege: 2 },
			fehler: null
		};
		const props = { data: { bericht, verlauf: { werte: [] } } };
		render(DatenSeite, props as unknown as ComponentProps<typeof DatenSeite>);
		expect(screen.getByText('27.11., 12:30')).toBeInTheDocument();
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(screen.getByText('28.11., 00:30')).toBeInTheDocument();
		expect(screen.getByText('27.11., 00:30')).toBeInTheDocument();
	});

	it('fmtDateTime: Dashboard-Abschnitt auf /pc', async () => {
		const betrieb = {
			modus: 'dienst',
			version: '0.0.0',
			commit: null,
			gestartet: '2026-11-27T08:00:00+01:00',
			einstellungen: 'dienst.toml',
			bote: 'verbunden',
			bote_seit: '2026-11-27T11:30:00Z',
			sitzung: { geht: null, inhaber_angemeldet: null, quelle: null, fehler: null, geprueft: null },
			fassung: 'build',
			fassung_gebaut: null,
			ordner: []
		} as unknown as Betrieb;
		render(DashboardStand, { laden: () => Promise.resolve(betrieb), zustand: null });
		expect(await screen.findByText(/verbunden seit 27\.11\.2026, 12:30/)).toBeInTheDocument();
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(screen.getByText(/verbunden seit 28\.11\.2026, 00:30/)).toBeInTheDocument();
	});

	it('fmtTagUhrzeit: Bau-Zeile einer Seite', () => {
		let zeile = (): string | undefined => undefined;
		const ende = $effect.root(() => {
			const z = $derived(
				bauAnzeige({ zustand: 'rot', letzte_gute: '2026-11-27T11:30:00Z' }, null, null, JETZT)
					?.zeile
			);
			zeile = () => z;
		});
		expect(zeile()).toBe(
			'Seite mit Fehler: zeigt die letzte gute Fassung von Fr 27.11., 12:30 Uhr'
		);
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(zeile()).toBe(
			'Seite mit Fehler: zeigt die letzte gute Fassung von Sa 28.11., 00:30 Uhr'
		);
		ende();
	});

	it('gitStandHinweis: Projektkarte', () => {
		const project = {
			slug: 'beispiel',
			title: 'Beispiel',
			status: 'aktiv',
			bedeutung: 'laeuft',
			last_change: '2026-11-27',
			summary: null,
			chat: null,
			backup: null,
			backup_bedeutung: null,
			backup_note: null,
			vorschau: null,
			order: 10,
			open: [],
			repos: [
				{
					path: 'x',
					label: 'beispiel',
					private: false,
					branch: 'main',
					commit_hash: 'abc',
					commit_date: '2026-11-27',
					commit_subject: 's',
					dirty_files: 0,
					error: null,
					stand: '2026-11-27T10:50:00Z'
				}
			],
			file_modified: null
		} as unknown as ProjectSummary;
		render(ProjectCard, { project, jetzt: JETZT });
		expect(screen.getByText('Git-Stand von 11:50')).toBeInTheDocument();
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(screen.getByText('Git-Stand von 27.11. 23:50')).toBeInTheDocument();
	});

	it('Vorgänge: „seit“ und die Zeit eines Schritts in der Zeile', async () => {
		const vorgang: Vorgang = {
			id: 'v-20261127-0001',
			art: 'antwort',
			slug: 'beispiel',
			titel: 'Eine Antwort',
			bezug: 'antwort:beispiel/x',
			zustand: 'offen',
			erstellt: '2026-11-26T05:30:00Z',
			geaendert: '2026-11-26T05:30:00Z',
			letzte_nr: 1,
			von: 'browser',
			notiz: null
		};
		const detail: VorgangDetail = {
			vorgang,
			ereignisse: [
				{
					nr: 1,
					zeit: '2026-11-27T10:50:00Z',
					vorgang: vorgang.id,
					art: 'antwort',
					slug: 'beispiel',
					zustand: 'offen',
					titel: vorgang.titel,
					bezug: vorgang.bezug,
					von: 'browser',
					notiz: null
				}
			]
		};
		render(VorgangZeile, { vorgang, laden: () => Promise.resolve(detail), jetzt: JETZT });
		await fireEvent.click(screen.getByRole('button', { name: /Eine Antwort/ }));
		expect(await screen.findByText('11:50 Uhr')).toBeInTheDocument();
		expect(screen.getByText('entstanden gestern')).toBeInTheDocument();
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(screen.getByText('gestern 23:50 Uhr')).toBeInTheDocument();
		expect(screen.getByText('entstanden am 26.11.2026')).toBeInTheDocument();
	});

	it('zeit/verlauf uhrzeit: Verlauf im Katalog', () => {
		render(ZeitVerlauf, {
			ereignisse: [
				{
					kennung: 'k:1',
					zeit: '2026-11-27T10:50:00Z',
					genauigkeit: 'minute',
					bis: null,
					titel: 'Netzteil getauscht',
					text: null,
					quelle: 'hand',
					art: 'erledigt',
					verweis: null
				}
			],
			jetzt: JETZT
		});
		expect(text()).toContain('11:50');
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(text()).toContain('23:50');
		expect(text()).not.toContain('11:50');
	});

	it('kurzZeit: Meldung im Unter-Dashboard', () => {
		const meldung: Aktualisierung = {
			quelle: 'dashboard',
			id: 'm-1',
			art: 'warnung',
			titel: 'Eine Meldung',
			zeit: '2026-11-27T12:30:00+01:00',
			ziel: '',
			gelesen: true,
			anzahl: 1,
			behoben: false,
			text: null
		};
		render(Meldung, { meldung });
		expect(text()).toContain('27.11. 12:30');
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(text()).toContain('28.11. 00:30');
	});

	it('zeitText eines Schritts: Schritte einer Arbeit', () => {
		const schritt: Schritt = {
			nr: 1,
			titel: 'Schritt 1',
			stand: 'laeuft',
			besitzer: 'chat:beispiel',
			wartet_auf: null,
			verweis: null,
			angeboten_an: null,
			fuer: null,
			seit: '2026-11-27T12:30:00+01:00',
			notiz: null,
			vorgang: null
		};
		render(Schritte, { titel: 'Etappe 1', schritte: [schritt], jetzt: JETZT.getTime() });
		expect(text()).toContain('seit 27.11., 12:30 Uhr');
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(text()).toContain('seit 28.11., 00:30 Uhr');
	});

	it('projektteile datum: Etappe im Plan', () => {
		const plan = projektPlan();
		const erste = plan.etappen[0];
		if (erste === undefined) throw new Error('Beispielplan ohne Etappe.');
		const etappe = {
			...erste,
			ab: null,
			stand: 'laeuft' as const,
			begonnen: '2026-11-27T12:30:00+01:00'
		};
		render(Plan, { plan: { ...plan, etappen: [etappe] }, jetzt: JETZT });
		expect(text()).toContain('läuft seit 27.11.');
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(text()).toContain('läuft seit 28.11.');
	});
});

describe('LaufendesJetzt am Blickort', () => {
	it('wechselt den Tag zur Mitternacht in Auckland', () => {
		vi.useFakeTimers();
		// Auckland Fr 27.11. 23:59:00 = Berlin 11:59.
		vi.setSystemTime(new Date('2026-11-27T10:59:00Z'));
		zoneSetzen(AUCKLAND);
		let jetzt: LaufendesJetzt | undefined;
		const ende = $effect.root(() => {
			jetzt = new LaufendesJetzt();
		});
		flushSync();
		if (jetzt === undefined) throw new Error('nicht gestartet');
		const uhr = jetzt;
		expect(tagInZone(uhr.wert)).toBe('2026-11-27');
		vi.advanceTimersByTime(61_000);
		expect(tagInZone(uhr.wert)).toBe('2026-11-28');
		ende();
	});

	it('ein Zonenwechsel stellt jetzt neu', () => {
		vi.useFakeTimers();
		vi.setSystemTime(new Date('2026-11-27T10:59:00Z'));
		let jetzt: LaufendesJetzt | undefined;
		const ende = $effect.root(() => {
			jetzt = new LaufendesJetzt();
		});
		flushSync();
		if (jetzt === undefined) throw new Error('nicht gestartet');
		const uhr = jetzt;
		// Ohne Zeitgeber vorgerückt: erst der Zonenwechsel nimmt die neue Zeit.
		vi.setSystemTime(new Date('2026-11-27T11:30:00Z'));
		expect(uhr.wert.getTime()).toBe(Date.parse('2026-11-27T10:59:00Z'));
		zoneSetzen(AUCKLAND);
		flushSync();
		expect(uhr.wert.getTime()).toBe(Date.parse('2026-11-27T11:30:00Z'));
		expect(tagInZone(uhr.wert)).toBe('2026-11-28');
		ende();
	});
});
