import { describe, expect, it } from 'vitest';
import { ANSTEHEND, kopfleiste, SEITEN } from './kopfleiste.ts';

const projekt = { title: 'Architektur des Dashboards', slug: 'bauplan' };
const neu = {
	kopf: {
		slug: 'bauplan',
		kopf: {
			title: 'Architektur des Dashboards',
			seiten: [
				{ name: 'zeitachse', title: 'Neue Zeitachse', order: 1 },
				{ name: 'notizen', title: 'Notizen', order: 2 }
			]
		}
	}
};

/** Nur die Texte: Glieder und Blatt. */
function weg(k: ReturnType<typeof kopfleiste>): string[] {
	return [...k.glieder.map((g) => g.text), k.blatt];
}

describe('Kopfleiste, Fassung 3: Route zu Brotkrumen und Markierung der Seitenleiste', () => {
	it('Leiste: oben Anstehend, abgesetzt Schlüssel, Hallo, Vorgänge, Daten, Anleitung, Zielbild, Entscheidungen; PC nicht', () => {
		expect(ANSTEHEND.label).toBe('Anstehend');
		expect(SEITEN.map((s) => s.label)).toEqual([
			'Schlüssel',
			'Hallo',
			'Vorgänge',
			'Daten',
			'Anleitung',
			'Zielbild',
			'Entscheidungen'
		]);
		expect(SEITEN.some((s) => s.label === 'PC')).toBe(false);
	});

	it('Startseite: nur das Blatt „Übersicht“, das Symbol ist hier (Ring), nichts markiert', () => {
		const k = kopfleiste('/', {}, {});
		expect(weg(k)).toEqual(['Übersicht']);
		expect(k.zuhause).toBe(true);
		expect(k.markiert).toEqual({ seite: null, projekt: null, unterseite: null });
	});

	it.each([
		['/anstehend', 'Anstehend', 'anstehend'],
		['/schluessel', 'Schlüssel', 'schluessel'],
		['/vorgaenge', 'Vorgänge', 'vorgaenge'],
		['/daten', 'Daten', 'daten'],
		['/hallo', 'Hallo', 'hallo'],
		['/anleitung', 'Anleitung', 'anleitung'],
		['/zielbild', 'Zielbild', 'zielbild'],
		['/entscheidungen', 'Entscheidungen', 'entscheidungen']
	])('%s: Blatt „%s“, in der Leiste markiert', (route, blatt, seite) => {
		const k = kopfleiste(route, {}, { irgendwas: 1 });
		expect(weg(k)).toEqual([blatt]);
		expect(k.markiert.seite).toBe(seite);
		expect(k.zuhause).toBe(false);
	});

	it('/pc: Blatt „PC“, in der Leiste nichts markiert (PC steht rechts oben)', () => {
		const k = kopfleiste('/pc', {}, {});
		expect(weg(k)).toEqual(['PC']);
		expect(k.markiert.seite).toBeNull();
	});

	it('Archivansicht (archiv-status): Projekte › Archiv, in der Leiste nichts markiert', () => {
		const k = kopfleiste('/projekte/archiviert', {}, {});
		expect(weg(k)).toEqual(['Projekte', 'Archiv']);
		expect(k.glieder[0]?.ziel).toEqual({ art: 'projekte' });
		expect(k.markiert).toEqual({ seite: null, projekt: null, unterseite: null });
		expect(SEITEN.some((s) => s.label === 'Archiv')).toBe(false);
	});

	it('Hallo bleibt „Hallo“, auch wenn der Brief einen eigenen Titel hat', () => {
		expect(kopfleiste('/hallo', {}, { hallo: { titel: 'Hello World' } }).blatt).toBe('Hallo');
	});

	it('Projektseite: Projekte › Projekttitel, das Projekt ist markiert', () => {
		const k = kopfleiste('/projects/[slug]', { slug: 'bauplan' }, projekt);
		expect(weg(k)).toEqual(['Projekte', 'Architektur des Dashboards']);
		expect(k.glieder[0]?.ziel).toEqual({ art: 'projekte' });
		expect(k.markiert).toEqual({ seite: null, projekt: 'bauplan', unterseite: null });
	});

	it('Unterseite (nur neue Form): Projekte › Projekt › Unterseite, das Glied führt zum Projekt', () => {
		const k = kopfleiste('/projects/[slug]/[name]', { slug: 'bauplan', name: 'zeitachse' }, neu);
		expect(weg(k)).toEqual(['Projekte', 'Architektur des Dashboards', 'Neue Zeitachse']);
		expect(k.glieder[1]?.ziel).toEqual({ art: 'projekt', slug: 'bauplan' });
		expect(k.markiert).toEqual({ seite: null, projekt: 'bauplan', unterseite: 'zeitachse' });
		expect(k.titel).toBe('Projekte · Architektur des Dashboards · Neue Zeitachse');
	});

	it('Unterseite, die es nicht gibt: ihr Name statt eines Titels', () => {
		expect(
			kopfleiste('/projects/[slug]/[name]', { slug: 'bauplan', name: 'fehlt' }, neu).blatt
		).toBe('fehlt');
	});

	it('Entwurf: Projekte › Projekt › Vorschau', () => {
		const k = kopfleiste(
			'/projects/[slug]/vorschau',
			{ slug: 'schaufenster' },
			{ title: 'Schaufenster' }
		);
		expect(weg(k)).toEqual(['Projekte', 'Schaufenster', 'Vorschau']);
		expect(k.markiert.unterseite).toBe('vorschau');
	});

	it('ohne Daten (etwa Fehlerseite): der Slug statt des Titels', () => {
		expect(kopfleiste('/projects/[slug]', { slug: 'bauplan' }, {}).blatt).toBe('bauplan');
		expect(
			kopfleiste('/projects/[slug]/vorschau', { slug: 'bauplan' }, null).glieder[1]?.text
		).toBe('bauplan');
	});

	it('Katalog und Logo-Entwürfe', () => {
		expect(weg(kopfleiste('/katalog', {}, {}))).toEqual(['Katalog']);
		const k = kopfleiste('/katalog/entwuerfe/logo', {}, {});
		expect(weg(k)).toEqual(['Katalog', 'Logo-Entwürfe']);
		expect(k.glieder[0]?.ziel).toEqual({ art: 'katalog' });
	});

	it('Anmelden und unbekannte Adresse: nichts markiert, kein Ring', () => {
		expect(weg(kopfleiste('/anmelden', {}, {}))).toEqual(['Anmelden']);
		const k = kopfleiste(null, {}, {});
		expect(weg(k)).toEqual(['Nicht gefunden']);
		expect(k.zuhause).toBe(false);
		expect(k.markiert.seite).toBeNull();
	});

	it('nirgends das Wort „Dashboard“ allein als Blatt', () => {
		for (const r of ['/', '/anstehend', '/pc', '/anmelden', null]) {
			expect(kopfleiste(r, {}, {}).blatt).not.toBe('Dashboard');
		}
	});
});
