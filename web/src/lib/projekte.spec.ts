import { describe, expect, it } from 'vitest';
import type { Bedeutung, ProjectSummary } from '$vertrag';
import { GRUPPEN, ansichtLesen, auswerten } from './projekte.ts';

function projekt(
	slug: string,
	bedeutung: Bedeutung,
	mehr: Partial<ProjectSummary> = {}
): ProjectSummary {
	const status: Record<Bedeutung, string> = {
		laeuft: 'aktiv',
		info: 'in arbeit',
		ok: 'fertig',
		wartet: 'pausiert',
		neutral: 'geplant',
		problem: 'problem'
	};
	return {
		slug,
		title: slug.toUpperCase(),
		status: status[bedeutung],
		bedeutung,
		last_change: '2026-10-01',
		summary: `Satz zu ${slug}`,
		chat: null,
		backup: null,
		backup_bedeutung: null,
		backup_note: '',
		vorschau: null,
		order: 100,
		open: [],
		repos: [],
		file_modified: null,
		...mehr
	};
}

const LISTE = [
	projekt('web', 'laeuft', { order: 20 }),
	projekt('postfach', 'laeuft', { order: 10, chat: 'Post' }),
	projekt('alt', 'ok', { last_change: '2026-10-30' }),
	projekt('pause', 'wartet'),
	projekt('kaputt', 'problem')
];

const ALLE = { suche: '', status: 'alle', sortierung: 'order' } as const;

describe('Startseite: auswerten (Etappe 3b)', () => {
	it('Spalten nach Bedeutung in der Reihenfolge der Zählzeile; leere fehlen', () => {
		const a = auswerten(LISTE, ALLE);
		expect(a.spalten.map((s) => s.wort)).toEqual(['aktiv', 'fertig', 'pausiert', 'mit Problem']);
		expect(a.spalten[0]?.projekte.map((p) => p.slug)).toEqual(['postfach', 'web']);
		expect(a.spalten.map((s) => s.anzahl)).toEqual([2, 1, 1, 1]);
		// Dieselben Zahlen wie die Zählzeile, jedes Projekt in genau einer Spalte.
		expect(a.spalten.map((s) => s.anzahl)).toEqual(a.zaehlung.gruppen.map((g) => g.anzahl));
		expect(a.spalten.flatMap((s) => s.projekte.map((p) => p.slug)).sort()).toEqual(
			a.gezeigt.map((p) => p.slug).sort()
		);
		expect(a.zeile).toBe('5 Projekte: 2 aktiv, 1 fertig, 1 pausiert, 1 mit Problem');
	});

	it('Suche und Filter gelten für Kacheln und Spalten gleich; gesamt bleibt die volle Zahl', () => {
		const a = auswerten(LISTE, { ...ALLE, suche: 'post' });
		expect(a.gezeigt.map((p) => p.slug)).toEqual(['postfach']);
		expect(a.spalten).toEqual([expect.objectContaining({ wort: 'aktiv', anzahl: 1, gesamt: 2 })]);
		const b = auswerten(LISTE, { ...ALLE, status: 'fertig' });
		expect(b.spalten.map((s) => s.wort)).toEqual(['fertig']);
		// Die Zählzeile zählt immer alle.
		expect(b.zeile).toBe(a.zeile);
	});

	it('sortiert nach Plan, Titel oder letzter Änderung; Statuswörter alphabetisch', () => {
		expect(
			auswerten(LISTE, ALLE)
				.gezeigt.map((p) => p.slug)
				.slice(0, 2)
		).toEqual(['postfach', 'web']);
		expect(auswerten(LISTE, { ...ALLE, sortierung: 'title' }).gezeigt[0]?.slug).toBe('alt');
		expect(auswerten(LISTE, { ...ALLE, sortierung: 'change' }).gezeigt[0]?.slug).toBe('alt');
		expect(auswerten(LISTE, ALLE).statusWoerter).toEqual([
			'aktiv',
			'fertig',
			'pausiert',
			'problem'
		]);
	});

	it('jede Bedeutung des Vertrags hat eine Spalte', () => {
		expect(GRUPPEN.map(([b]) => b).sort()).toEqual(
			['info', 'laeuft', 'neutral', 'ok', 'problem', 'wartet'].sort()
		);
	});

	it('ohne Speicher (hier: Node ohne localStorage) gilt „Kacheln“', () => {
		expect(ansichtLesen()).toBe('kacheln');
	});
});
