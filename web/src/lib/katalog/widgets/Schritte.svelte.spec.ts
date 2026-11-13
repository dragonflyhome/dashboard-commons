import { render, screen, within } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { eintrag } from '#lib/anstehend.beispiel.ts';
import type { AnstehendEintrag, Schritt } from '$vertrag';
import Arbeiten from './Arbeiten.svelte';
import Schritte from './Schritte.svelte';
import { SATZ_STILLSTAND, schritteKurz, verweisZiel, wartetAufWort } from './worte.ts';

/** Zweig teilschritte, T4: die Schritte einer Arbeit im Kasten Arbeiten. */
const schritt = (nr: number, teil: Partial<Schritt> = {}): Schritt => ({
	nr,
	titel: `Schritt ${String(nr)}`,
	stand: 'als-naechstes',
	besitzer: null,
	wartet_auf: null,
	verweis: null,
	angeboten_an: null,
	fuer: null,
	seit: '2026-11-05T09:00:00+01:00',
	notiz: null,
	vorgang: null,
	...teil
});

const fertig = (nr: number): Schritt =>
	schritt(nr, { stand: 'fertig', besitzer: 'chat:dashboard' });

/** 05.11.2026, 12:00 Ortszeit (+01:00). */
const JETZT = Date.parse('2026-11-05T12:00:00+01:00');

function arbeitMit(
	schritte: Schritt[] | undefined,
	teil: Partial<Omit<AnstehendEintrag, 'stufe'>> = {}
) {
	return render(Arbeiten, {
		gruppen: [
			{
				wort: 'läuft',
				eintraege: [
					eintrag({
						art: 'arbeit',
						id: 'ar-1',
						titel: 'Etappe 6',
						...teil,
						...(schritte ? { schritte } : {})
					})
				],
				weitere: 0
			}
		]
	});
}

describe('Schritte einer Arbeit (T4)', () => {
	it('zugeklappt in Worten: „3 von 7 fertig · dran: Chat architektur“, aufgeklappt eine beschriftete Liste', () => {
		const sieben = [
			fertig(1),
			fertig(2),
			fertig(3),
			schritt(4, { stand: 'laeuft', besitzer: 'chat:architektur' }),
			schritt(5),
			schritt(6),
			schritt(7)
		];
		arbeitMit(sieben);
		const kopf = document.querySelector('#arbeit-ar-1 > summary');
		expect(kopf).toHaveTextContent('Etappe 6, 3 von 7 fertig · dran: Chat architektur');
		// Der Trenner ist nur für Vorleseprogramme (T5, Auflage aus T4), steht aber im Fluss
		// direkt hinter dem Titel: .nur-vorlesen ist absolut, Edge zeigte in innerText ein loses Komma.
		const trenner = kopf?.querySelector('.titel + .trenner');
		expect(trenner?.textContent).toBe(', ');
		expect(trenner).not.toHaveClass('nur-vorlesen');
		const liste = screen.getByRole('list', { name: 'Schritte von „Etappe 6“' });
		const punkte = within(liste).getAllByRole('listitem');
		expect(punkte).toHaveLength(7);
		expect(punkte[0]).toHaveTextContent(
			'1. Schritt 1 fertig · erledigt von Chat dashboard · am 05.11., 09:00 Uhr'
		);
		expect(punkte[3]).toHaveTextContent('4. Schritt 4 läuft · dran: Chat architektur');
		expect(punkte[4]).toHaveTextContent('5. Schritt 5 als Nächstes · noch frei');
		// Farbe nur zusätzlich, keine Fortschrittsleiste, keine Knöpfe.
		expect(punkte[3]).toHaveClass('b-laeuft');
		expect(screen.queryByRole('progressbar')).toBeNull();
		expect(screen.queryByRole('button')).toBeNull();
	});

	it('angeboten, noch nicht übernommen: in der Zeile und am Schritt', () => {
		const s = [fertig(1), schritt(2, { angeboten_an: 'chat:architektur' })];
		expect(schritteKurz(s)).toBe(
			'1 von 2 fertig · angeboten an Chat architektur, noch nicht übernommen'
		);
		arbeitMit(s);
		const punkte = within(screen.getByRole('list', { name: /Schritte von/ })).getAllByRole(
			'listitem'
		);
		expect(punkte[1]).toHaveTextContent(
			'als Nächstes · angeboten an Chat architektur, noch nicht übernommen'
		);
	});

	it('worauf er wartet: Schritt n, du (Link nur zur offenen Frage oder Aufgabe), jemand anderes', () => {
		render(Schritte, {
			titel: 'Etappe 6',
			jetzt: JETZT,
			offeneVerweise: ['a-20261105-0001'],
			schritte: [
				schritt(1, {
					stand: 'wartet',
					besitzer: 'chat:dashboard',
					wartet_auf: 'inhaber',
					verweis: 'a-20261105-0001'
				}),
				schritt(2, { stand: 'wartet', besitzer: 'chat:architektur', wartet_auf: 'schritt:1' }),
				schritt(3, { stand: 'wartet', besitzer: 'chat:hallo', wartet_auf: 'extern' }),
				schritt(4, {
					stand: 'wartet',
					besitzer: 'chat:hallo',
					wartet_auf: 'inhaber',
					verweis: 'schlusssatz'
				})
			]
		});
		const punkte = screen.getAllByRole('listitem') as [
			HTMLElement,
			HTMLElement,
			HTMLElement,
			HTMLElement
		];
		expect(punkte[0]).toHaveTextContent('wartet auf dich · dran: Chat dashboard');
		expect(within(punkte[0]).getByRole('link', { name: 'zur Aufgabe' })).toHaveAttribute(
			'href',
			'#aufgabe-a-20261105-0001'
		);
		expect(punkte[1]).toHaveTextContent('wartet auf Schritt 1 · dran: Chat architektur');
		expect(punkte[2]).toHaveTextContent('wartet auf jemand anderen');
		// Schritt 4 wartet auch auf dich, sein Verweis ist aber nicht der offene: kein Link ins Leere.
		expect(punkte[3]).toHaveTextContent('wartet auf dich · dran: Chat hallo');
		expect(screen.getAllByRole('link')).toHaveLength(1);
		expect(wartetAufWort('schritt:12')).toBe('wartet auf Schritt 12');
		expect(verweisZiel('schlusssatz')).toEqual({ anker: 'frage-schlusssatz', wort: 'zur Frage' });
		expect(verweisZiel('a-2026-1')).toEqual({ anker: 'frage-a-2026-1', wort: 'zur Frage' });
		expect(verweisZiel('a-20261105-12345')).toEqual({
			anker: 'aufgabe-a-20261105-12345',
			wort: 'zur Aufgabe'
		});
	});

	it('wartet auf dich in der Arbeit: Link nur zum offenen Verweis, „Stand“ ohne Wiederholung', () => {
		const wartend = [
			fertig(1),
			schritt(2, {
				stand: 'wartet',
				besitzer: 'chat:hallo',
				wartet_auf: 'inhaber',
				verweis: 'schlusssatz'
			})
		];
		const teil = { zustand: 'wartet', wartet_auf: 'inhaber', verweis: 'schlusssatz' };
		const stand = () => screen.getByText(/^\s*Stand:/);

		const mit = arbeitMit(wartend, {
			...teil,
			vermerk: 'wartet auf deine Antwort',
			offene_verweise: ['schlusssatz']
		});
		expect(screen.getByRole('link', { name: 'zur Frage' })).toHaveAttribute(
			'href',
			'#frage-schlusssatz'
		);
		expect(screen.getByRole('list', { name: /Schritte von/ })).toHaveTextContent(
			'wartet auf dich · dran: Chat hallo'
		);
		expect(stand()).toHaveTextContent(/^Stand: wartet$/);
		mit.unmount();

		// Frage schon beantwortet: nicht unter den offenen Verweisen, also kein Link ins Leere.
		const ohne = arbeitMit(wartend, teil);
		expect(screen.queryByRole('link')).toBeNull();
		ohne.unmount();

		// Ohne Schritte bleibt die alte Zeile, wie sie war.
		arbeitMit(undefined, teil);
		expect(stand()).toHaveTextContent('Stand: wartet, wartet auf dich');
	});

	it('zwei Schritte warten auf dich: Vermerk und Link je Schritt nach seinem eigenen Verweis (T5)', () => {
		const zwei = [
			schritt(1, {
				stand: 'wartet',
				besitzer: 'chat:dashboard',
				wartet_auf: 'inhaber',
				verweis: 'schlusssatz'
			}),
			schritt(2, {
				stand: 'wartet',
				besitzer: 'chat:dashboard',
				wartet_auf: 'inhaber',
				verweis: 'a-20261105-0031'
			})
		];
		const teil = {
			zustand: 'wartet',
			wartet_auf: 'inhaber',
			verweis: 'schlusssatz',
			vermerk: 'wartet auf deine Antwort'
		};
		const punkte = () =>
			within(screen.getByRole('list', { name: /Schritte von/ })).getAllByRole('listitem') as [
				HTMLElement,
				HTMLElement
			];

		// (i) Die Frage des ersten ist beantwortet, die Aufgabe des zweiten offen.
		const eins = arbeitMit(zwei, { ...teil, offene_verweise: ['a-20261105-0031'] });
		expect(screen.getByText('wartet auf deine Antwort')).toBeInTheDocument();
		expect(within(punkte()[0]).queryByRole('link')).toBeNull();
		expect(within(punkte()[1]).getByRole('link', { name: 'zur Aufgabe' })).toHaveAttribute(
			'href',
			'#aufgabe-a-20261105-0031'
		);
		eins.unmount();

		// (ii) Beide offen: beide mit Link.
		arbeitMit(zwei, { ...teil, offene_verweise: ['schlusssatz', 'a-20261105-0031'] });
		expect(within(punkte()[0]).getByRole('link', { name: 'zur Frage' })).toHaveAttribute(
			'href',
			'#frage-schlusssatz'
		);
		expect(within(punkte()[1]).getByRole('link', { name: 'zur Aufgabe' })).toBeInTheDocument();
		expect(screen.getAllByRole('link')).toHaveLength(2);
	});

	describe('auf einer offen gelassenen Seite', () => {
		afterEach(() => {
			vi.useRealTimers();
		});

		it('erscheint der 24-h-Satz ohne Neuladen', () => {
			vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'Date'] });
			vi.setSystemTime(JETZT);
			render(Schritte, {
				titel: 'Etappe 6',
				// 30 Sekunden vor der 24-h-Grenze.
				schritte: [
					schritt(1, {
						stand: 'laeuft',
						besitzer: 'chat:dashboard',
						seit: '2026-11-04T12:00:30+01:00'
					})
				]
			});
			expect(screen.queryByText(SATZ_STILLSTAND)).toBeNull();
			vi.advanceTimersByTime(60_000);
			flushSync();
			expect(screen.getByText(SATZ_STILLSTAND)).toBeInTheDocument();
		});
	});

	it('„seit über einem Tag unverändert“ nur bei Stillstand über 24 Stunden', () => {
		const gestern = '2026-11-04T11:59:00+01:00';
		render(Schritte, {
			titel: 'Etappe 6',
			jetzt: JETZT,
			schritte: [
				schritt(1, { stand: 'laeuft', besitzer: 'chat:dashboard', seit: gestern }),
				schritt(2, {
					stand: 'laeuft',
					besitzer: 'chat:dashboard',
					seit: '2026-11-04T12:01:00+01:00'
				}),
				schritt(3, { angeboten_an: 'chat:architektur', seit: gestern }),
				schritt(4, { seit: gestern }),
				schritt(5, { stand: 'fertig', besitzer: 'chat:dashboard', seit: gestern })
			]
		});
		const still = screen
			.getAllByRole('listitem')
			.map((li) => li.textContent.includes(SATZ_STILLSTAND));
		expect(still).toEqual([true, false, true, false, false]);
	});

	it('ohne Schritte unverändert: keine Zeile, keine Liste', () => {
		arbeitMit(undefined);
		// Kein ▸ neben dem Dreieck des Aufklappers (T5, Hinweis aus T4).
		expect(document.querySelector('#arbeit-ar-1 > summary')?.textContent.trim()).toBe('Etappe 6');
		expect(screen.queryByRole('list', { name: /Schritte von/ })).toBeNull();
		expect(document.querySelector('.kurz')).toBeNull();
	});
});
