import { describe, expect, it } from 'vitest';
import { eintrag } from './anstehend.beispiel.ts';
import {
	auswerten,
	behobenUngelesen,
	glocke,
	istRechteMeldung,
	wartetJeProjekt
} from './anstehend.ts';
import { beispielListe } from './anstehend.beispiel.ts';
import type { AnstehendEintrag, Meldung } from '$vertrag';

// Kopfleiste, Fassung 3: Glocke und Zähler „wartet“ aus der einen Auswertungsstelle.
function m(id: string, zuletzt: string, schluessel: string): AnstehendEintrag {
	return eintrag({
		art: 'meldung',
		id,
		titel: id,
		wartet_auf_dich: false,
		meldung: { art: 'fehler', anzahl: 1, zuletzt, text: null, system: false, schluessel },
		erstellt: zuletzt
	});
}

describe('Glocke und Zähler „wartet“ (anstehend.ts)', () => {
	it('Rechte- und Zugriffsmeldungen am Schlüssel erkannt', () => {
		expect(istRechteMeldung('dashboard/lesen:docs')).toBe(true);
		expect(istRechteMeldung('dashboard/lesen:projects')).toBe(true);
		expect(istRechteMeldung('dashboard/bote-fehlt')).toBe(false);
		expect(istRechteMeldung('schaufenster/bau-rot')).toBe(false);
		expect(istRechteMeldung(undefined)).toBe(false);
	});

	it('glocke: Meldungen neueste zuerst, Hinweise getrennt und ohne Zahl', () => {
		const g = glocke([
			m('m-1', '2026-10-30T08:00:00+01:00', 'dashboard/pruefung-rot'),
			m('m-2', '2026-10-30T10:00:00+01:00', 'dashboard/lesen:docs'),
			m('m-3', '2026-10-30T09:00:00+01:00', 'schaufenster/bau-rot'),
			eintrag({ id: 'a-1' })
		]);
		expect(g.zahl).toBe(2);
		expect(g.meldungen.map((e) => e.id)).toEqual(['m-3', 'm-1']);
		expect(g.hinweise.map((e) => e.id)).toEqual(['m-2']);
	});

	it('behobenUngelesen: nur behoben und ungelesen, nicht im Archiv, neueste zuerst', () => {
		const roh = (id: string, behoben: string | null, gelesen: string | null, zuletzt: string) =>
			({ id, behoben, gelesen, archiviert: null, zuletzt }) as Meldung;
		expect(
			behobenUngelesen([
				roh('a', '2026-10-30T09:00:00+01:00', null, '2026-10-30T08:00:00+01:00'),
				roh('b', null, null, '2026-10-30T09:00:00+01:00'),
				roh(
					'c',
					'2026-10-30T09:00:00+01:00',
					'2026-10-30T09:30:00+01:00',
					'2026-10-30T09:00:00+01:00'
				),
				roh('d', '2026-10-30T09:00:00+01:00', null, '2026-10-30T10:00:00+01:00')
			]).map((x) => x.id)
		).toEqual(['d', 'a']);
	});

	it('wartetJeProjekt: nach dem Fakt wartet_auf_dich, aus der ausgewerteten Liste', () => {
		const a = auswerten(beispielListe());
		const z = wartetJeProjekt(a.eintraege);
		const summe = Object.values(z).reduce((s, n) => s + n, 0);
		expect(summe).toBe(a.zaehler.wartet_auf_dich);
		expect(Object.keys(z).length).toBeGreaterThan(0);
	});
});
