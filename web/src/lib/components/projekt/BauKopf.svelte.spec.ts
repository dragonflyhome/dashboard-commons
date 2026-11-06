/**
 * Etappe 4c: Bau-Stand im Kopf der Projektseite. Nur noch die
 * Statuszeile: keine Zeitzeile, kein Knopf zum
 * Bauen, keine Ansage dazu.
 */
import { render, screen } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { describe, expect, it } from 'vitest';
import type { ProjectSummary } from '$vertrag';
import { BauLage } from '#lib/bau.svelte.ts';
import BauKopf from './BauKopf.svelte';

/** Samstag, 31.10.2026, 14:05 Ortszeit: die letzte gute Fassung. */
const GUT = new Date(2026, 9, 31, 14, 5).toISOString();

function projekt(slug: string, bau?: ProjectSummary['bau']): ProjectSummary {
	return {
		slug,
		title: slug,
		status: 'aktiv',
		bedeutung: 'laeuft',
		last_change: '2026-10-31',
		summary: '',
		chat: null,
		backup: null,
		backup_bedeutung: null,
		backup_note: '',
		vorschau: null,
		order: 1,
		open: [],
		repos: [],
		file_modified: null,
		...(bau === undefined ? {} : { bau })
	};
}

/** Ein Stand, dessen `fetch` jede Adresse merkt (angemeldet: 200 ohne Sätze). */
function lage(aufrufe: string[] = []): BauLage {
	return new BauLage((url: string | URL | Request, init?: RequestInit) => {
		const adresse = typeof url === 'string' ? url : url instanceof URL ? url.href : url.url;
		aufrufe.push(`${init?.method ?? 'GET'} ${adresse}`);
		return Promise.resolve(
			new Response(JSON.stringify({ projekte: [] }), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	});
}

/** Kein Rest der Zeile, des Kreispfeils oder seiner Ansage. */
function ohneZeileUndKreispfeil(): void {
	expect(document.querySelector('.generiert')).toBeNull();
	expect(screen.queryByText(/generiert/)).toBeNull();
	expect(screen.queryByText(/Neugenerierung/)).toBeNull();
	expect(screen.queryByRole('button')).toBeNull();
	expect(document.querySelector('[data-neu-generieren]')).toBeNull();
	expect(document.querySelector('.bau svg')).toBeNull();
	expect(document.querySelector('.bau [role="status"]')).toBeNull();
	expect(document.querySelector('.bau time')).toBeNull();
}

describe('BauKopf (Etappe 4c)', () => {
	it('rot: „zeigt die letzte gute Fassung von …“ und, angemeldet, der Satz', () => {
		const l = lage();
		l.liste([projekt('p', { zustand: 'rot', letzte_gute: GUT, gebaut_um: GUT })]);
		l.saetze = { p: 'Die neue Fassung der Seite hat einen Fehler.' };
		render(BauKopf, { slug: 'p', lage: l });
		expect(
			screen.getByText(/^Seite mit Fehler: zeigt die letzte gute Fassung von \S+ 31\.10\./)
		).toBeInTheDocument();
		expect(screen.getByText('Die neue Fassung der Seite hat einen Fehler.')).toBeInTheDocument();
		expect(document.querySelector('[data-bau="rot"]')).not.toBeNull();
		ohneZeileUndKreispfeil();
	});

	it('ohne eigene Seite nichts; grün nur „Seite in Ordnung“', () => {
		const l = lage();
		l.liste([
			projekt('nur-md'),
			projekt('p', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT })
		]);
		const { unmount } = render(BauKopf, { slug: 'nur-md', lage: l });
		expect(document.querySelector('.bau')).toBeNull();
		unmount();
		render(BauKopf, { slug: 'p', lage: l });
		expect(screen.getByText(/^Seite in Ordnung$/)).toBeInTheDocument();
		ohneZeileUndKreispfeil();
	});

	it('läuft, noch nicht gebaut, nicht lesbar, ausgeschlossen: nur die Statuszeile', () => {
		const faelle: [ProjectSummary['bau'], string][] = [
			[{ zustand: 'laeuft', letzte_gute: GUT, gebaut_um: GUT }, 'Seite wird gebaut …'],
			[{ zustand: 'noch_nicht', letzte_gute: null }, 'Seite noch nicht gebaut'],
			[{ zustand: 'unbekannt', letzte_gute: null }, 'Bau-Stand gerade nicht lesbar'],
			[{ zustand: 'rot', letzte_gute: null }, 'Seite mit Fehler, gerade nicht zu sehen']
		];
		for (const [bau, zeile] of faelle) {
			const l = lage();
			l.liste([projekt('p', bau)]);
			const { unmount } = render(BauKopf, { slug: 'p', lage: l });
			expect(screen.getByText(zeile)).toBeInTheDocument();
			ohneZeileUndKreispfeil();
			unmount();
		}
	});

	it('angemeldet (die Sätze kamen): trotzdem kein Kreispfeil, kein Bau bestellt', async () => {
		const aufrufe: string[] = [];
		const l = lage(aufrufe);
		l.liste([projekt('p', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT })]);
		await l.laden();
		render(BauKopf, { slug: 'p', lage: l });
		ohneZeileUndKreispfeil();
		expect(aufrufe).toEqual(['GET /api/bauen']);
	});
});

describe('BauKopf: nicht geprüft (Zweig nicht-geprueft-sichtbar)', () => {
	const MIT =
		'Zu sehen ist die letzte geprüfte Fassung; die neue Fassung ist noch nicht geprüft. Der nächste Bau prüft sie noch einmal.';
	const OHNE =
		'Die Seite ist noch nicht geprüft und darum noch nicht zu sehen. Der nächste Bau prüft sie noch einmal.';
	// Angemeldet mit Kopie wie bei rot mit der Zeit der Fassung (Gegenprüfung, Fund 3).
	const faelle: [string, ProjectSummary['bau'], string, RegExp][] = [
		[
			'mit Kopie',
			{ zustand: 'nicht_geprueft', letzte_gute: GUT, gebaut_um: GUT },
			MIT,
			/^Seite noch nicht geprüft: zeigt die letzte geprüfte Fassung von .*31\.10\..*14:05/
		],
		[
			'ohne Kopie',
			{ zustand: 'nicht_geprueft', letzte_gute: null },
			OHNE,
			/^Seite noch nicht geprüft$/
		]
	];

	for (const [name, bau, satz, zeile] of faelle) {
		it(`${name}, angemeldet: Zustand und darunter der Satz, kein Alarm`, () => {
			const l = lage();
			l.liste([projekt('p', bau)]);
			l.saetze = { p: satz };
			render(BauKopf, { slug: 'p', lage: l });
			expect(screen.getByText(zeile)).toBeInTheDocument();
			expect(screen.getByText(satz)).toBeInTheDocument();
			expect(screen.queryByText(/in Ordnung|Fehler/)).toBeNull();
			expect(document.querySelector('[data-bau="nicht_geprueft"]')).not.toBeNull();
			expect(document.querySelector('.bau [aria-live], .bau [role="alert"]')).toBeNull();
			ohneZeileUndKreispfeil();
		});

		it(`${name}, ohne Anmeldung: nur der Zustand, ohne Zeit`, () => {
			const l = lage();
			l.liste([projekt('p', bau)]);
			render(BauKopf, { slug: 'p', lage: l });
			expect(document.querySelector('.bau')?.textContent.trim()).toBe('Seite noch nicht geprüft');
			expect(document.querySelector('.satz')).toBeNull();
			expect(document.querySelector('.bau [aria-live], .bau [role="alert"]')).toBeNull();
			ohneZeileUndKreispfeil();
		});
	}

	it('Farbe nur zusätzlich: nicht die Farbe für Fehler', () => {
		const l = lage();
		l.liste([projekt('p', { zustand: 'nicht_geprueft', letzte_gute: GUT, gebaut_um: GUT })]);
		expect(l.anzeige('p')?.bedeutung).toBe('wartet');
		expect(l.anzeige('p')?.wort).toBe('Seite noch nicht geprüft');
	});

	it('ein Ereignis grün zeigt bis zur nächsten Liste „wird gebaut“, nie „in Ordnung“', () => {
		const l = lage();
		l.liste([projekt('p', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT })]);
		l.ereignis({ slug: 'p', zustand: 'gruen', meldung: 'x', gescheitert: false });
		render(BauKopf, { slug: 'p', lage: l });
		expect(document.querySelector('[data-bau="laeuft"]')).not.toBeNull();
		expect(screen.queryByText(/in Ordnung/)).toBeNull();
		// Die nächste Liste sagt, was gilt.
		l.liste([projekt('p', { zustand: 'nicht_geprueft', letzte_gute: GUT, gebaut_um: GUT })]);
		expect(l.anzeige('p')?.zustand).toBe('nicht_geprueft');
		// Rot aus dem Ereignis gilt weiter vor der Liste.
		l.ereignis({ slug: 'p', zustand: 'rot', meldung: 'x', gescheitert: false });
		expect(l.anzeige('p')?.zustand).toBe('rot');
	});
});

describe('BauKopf mit dem Stand des Projekts', () => {
	it('grün und Karte „Stand“ rot: kein „Seite in Ordnung“, sondern „Seite gebaut“', () => {
		const l = lage();
		l.liste([projekt('p', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT })]);
		l.standMelden('p', true);
		render(BauKopf, { slug: 'p', lage: l });
		expect(screen.queryByText(/Seite in Ordnung/)).toBeNull();
		expect(
			screen.getByText(/^Seite gebaut \(Fassung von .*offen ist etwas, siehe „Stand“\.$/)
		).toBeInTheDocument();
		// Die Karte (Leiste, Startseite) behält ihr Wort: Dort steht keine rote Karte daneben.
		expect(l.anzeige('p')?.wort).toBe('Seite in Ordnung');
	});
	it('Stand wieder nicht rot: „Seite in Ordnung“', () => {
		const l = lage();
		l.liste([projekt('p', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT })]);
		l.standMelden('p', true);
		l.standMelden('p', false);
		render(BauKopf, { slug: 'p', lage: l });
		expect(screen.getByText(/^Seite in Ordnung$/)).toBeInTheDocument();
	});
});

describe('BauKopf: Schrittzähler (Bau-Tempo, Zweig C)', () => {
	/** Was Vorlese-Programme von selbst melden würden: Live-Regionen und Fortschrittsbalken. */
	function ansagbar(): Element[] {
		return [
			...document.querySelectorAll(
				'[aria-live], [role="status"], [role="alert"], [role="log"], [role="timer"], [role="marquee"], [role="progressbar"], progress'
			)
		];
	}

	function laeuft(): BauLage {
		const l = lage();
		l.liste([projekt('p', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT })]);
		l.ereignis({ slug: 'p', zustand: 'laeuft', meldung: 'x', gescheitert: false });
		return l;
	}

	it('Gegenprobe: die Suche findet Live-Region und Fortschrittsbalken', () => {
		document.body.insertAdjacentHTML(
			'beforeend',
			'<div id="gegen"><span role="status">a</span><div role="progressbar"></div><p aria-live="polite">b</p></div>'
		);
		expect(ansagbar()).toHaveLength(3);
		document.getElementById('gegen')?.remove();
	});

	it('angemeldet: „prüft Projekt x von y“ sichtbar, ohne aria-live, role=status oder progressbar', () => {
		const l = laeuft();
		l.fortschritt = { schritt: 'pruefung', fertig: 6, gesamt: 21 };
		render(BauKopf, { slug: 'p', lage: l });
		expect(screen.getByText('Seite wird gebaut …')).toBeInTheDocument();
		expect(screen.getByText('prüft Projekt 7 von 21')).toBeVisible();
		expect(ansagbar()).toEqual([]);
		ohneZeileUndKreispfeil();
	});

	it('nach mehreren Ticks keine Ansage; Sammelprüfung als „prüft n Seiten“; am Ende weg', () => {
		const l = laeuft();
		render(BauKopf, { slug: 'p', lage: l });
		for (let fertig = 0; fertig < 5; fertig++) {
			l.fortschritt = { schritt: 'pruefung', fertig, gesamt: 5 };
			flushSync();
			expect(document.querySelector('[data-bau-schritt]')?.textContent).toBe(
				`prüft Projekt ${String(fertig + 1)} von 5`
			);
			expect(ansagbar()).toEqual([]);
		}
		l.fortschritt = { schritt: 'sammelpruefung', fertig: 0, gesamt: 21 };
		flushSync();
		expect(screen.getByText('prüft 21 Seiten')).toBeInTheDocument();
		l.ereignis({ slug: 'p', zustand: 'gruen', meldung: 'x', gescheitert: false });
		flushSync();
		expect(document.querySelector('[data-bau-schritt]')).toBeNull();
		expect(l.fortschritt).toBeNull();
	});

	it.each(['gruen', 'rot'] as const)(
		'nach %s: ein verspäteter Zähler steht beim nächsten Bau nicht im Kopf',
		(ende) => {
			const l = laeuft();
			render(BauKopf, { slug: 'p', lage: l });
			l.fortschritt = { schritt: 'pruefung', fertig: 2, gesamt: 3 };
			l.ereignis({ slug: 'p', zustand: ende, meldung: 'x', gescheitert: false });
			expect(l.fortschritt).toBeNull();
			// Zwei Kanäle: Der letzte Zähler kann nach dem Ende ankommen.
			l.fortschritt = { schritt: 'vite_build', fertig: 0, gesamt: 1 };
			l.ereignis({ slug: 'p', zustand: 'laeuft', meldung: 'x', gescheitert: false });
			flushSync();
			expect(screen.getByText('Seite wird gebaut …')).toBeInTheDocument();
			expect(document.querySelector('[data-bau-schritt]')).toBeNull();
		}
	);

	it('Ende verpasst (Liste ohne laufenden Bau): beim nächsten Bau aus der Liste kein alter Zähler', () => {
		const l = laeuft();
		l.fortschritt = { schritt: 'pruefung', fertig: 6, gesamt: 21 };
		l.liste([projekt('p', { zustand: 'gruen', letzte_gute: GUT, gebaut_um: GUT })]);
		l.liste([projekt('p', { zustand: 'laeuft', letzte_gute: GUT, gebaut_um: GUT })]);
		render(BauKopf, { slug: 'p', lage: l });
		expect(screen.getByText('Seite wird gebaut …')).toBeInTheDocument();
		expect(document.querySelector('[data-bau-schritt]')).toBeNull();
	});

	it('Liste mit laufendem Bau: der Zähler bleibt', () => {
		const l = laeuft();
		l.fortschritt = { schritt: 'gesamtpruefung', fertig: 0, gesamt: 1 };
		l.liste([projekt('p', { zustand: 'laeuft', letzte_gute: GUT, gebaut_um: GUT })]);
		render(BauKopf, { slug: 'p', lage: l });
		expect(screen.getByText('prüft alles zusammen und baut')).toBeInTheDocument();
	});

	it('ohne Anmeldung (Bau-Stand nur aus der Liste, kein Fortschritt): kein Zähler', () => {
		const l = lage();
		l.liste([projekt('p', { zustand: 'laeuft', letzte_gute: GUT, gebaut_um: GUT })]);
		render(BauKopf, { slug: 'p', lage: l });
		expect(screen.getByText('Seite wird gebaut …')).toBeInTheDocument();
		expect(document.querySelector('[data-bau-schritt]')).toBeNull();
		expect(screen.queryByText(/prüft/)).toBeNull();
	});
});
