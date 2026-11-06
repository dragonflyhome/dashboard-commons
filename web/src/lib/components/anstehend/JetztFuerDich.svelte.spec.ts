import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import JetztFuerDich from './JetztFuerDich.svelte';
import { beispielListe, eintrag } from '#lib/anstehend.beispiel.ts';
import type { AnstehendListe } from '$vertrag';

function liste(eintraege: AnstehendListe['eintraege']): AnstehendListe {
	return { ...beispielListe(), eintraege };
}

function karte(): HTMLElement {
	return screen.getByRole('region', { name: 'Jetzt für dich' });
}

describe('Jetzt für dich (Startseite)', () => {
	it('je Projekt die Frage mit Vermerk und Arbeiten unter Jetzt, die auf dich warten, mit Links', () => {
		const frage = eintrag({
			art: 'frage',
			id: 'schlusssatz',
			projekt: 'beispiel',
			projekt_titel: 'Beispiel',
			titel: 'Welcher Schlusssatz passt?',
			stufe: 'jetzt',
			vermerk: 'noch 2 Fragen warten'
		});
		const arbeit = eintrag({
			art: 'arbeit',
			id: 'ar-1',
			titel: 'Bau reparieren',
			stufe: 'jetzt',
			frist: '2026-10-29',
			tage_bis: 0,
			zustand: 'wartet',
			wartet_auf: 'inhaber'
		});
		// Läuft nur (wartet nicht auf dich): steht nicht hier (Auflage aus A1).
		const laeuft = eintrag({ art: 'arbeit', id: 'ar-2', titel: 'Läuft nur', stufe: 'jetzt' });
		const aufgabe = eintrag({ id: 'a-1', titel: 'Beim Probe-Dienst anmelden', stufe: 'jetzt' });
		render(JetztFuerDich, { liste: liste([aufgabe, laeuft, arbeit, frage]) });
		const k = karte();
		expect([...k.querySelectorAll('h3')].map((h) => h.textContent)).toEqual([
			'Dashboard',
			'Beispiel'
		]);
		const f = within(k).getByRole('link', { name: 'Welcher Schlusssatz passt?' });
		expect(f.getAttribute('href')).toBe('/projects/beispiel#frage-schlusssatz');
		expect(within(k).getByText(/noch 2 Fragen warten/)).toBeInTheDocument();
		expect(within(k).getByRole('link', { name: 'Bau reparieren' }).getAttribute('href')).toBe(
			'/projects/dashboard#arbeit-ar-1'
		);
		expect(within(k).getByText(/Frist heute/)).toBeInTheDocument();
		// Aufgaben stehen schon in „Für dich“.
		expect(within(k).queryByText('Beim Probe-Dienst anmelden')).toBeNull();
		expect(within(k).queryByText('Läuft nur')).toBeNull();
		expect(within(k).getByRole('link', { name: 'Alles in „Anstehend“' }).getAttribute('href')).toBe(
			'/anstehend'
		);
	});

	it('höchstens fünf Projekte, dann „und N weitere“', () => {
		const fragen = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map((p) =>
			eintrag({
				art: 'frage',
				id: `f-${p}`,
				projekt: p,
				projekt_titel: p.toUpperCase(),
				stufe: 'jetzt'
			})
		);
		render(JetztFuerDich, { liste: liste(fragen) });
		expect(karte().querySelectorAll('h3')).toHaveLength(5);
		expect(screen.getByRole('link', { name: 'und 2 weitere Projekte' })).toHaveAttribute(
			'href',
			'/anstehend'
		);
	});

	it('fehlt still: ohne Liste (ohne Anmeldung, Fehler) und ohne Einträge', () => {
		const { unmount } = render(JetztFuerDich, { liste: null });
		expect(screen.queryByRole('region', { name: 'Jetzt für dich' })).toBeNull();
		unmount();
		// Nur eine Aufgabe und eine Arbeit, die nicht jetzt ist: nichts für diese Karte.
		render(JetztFuerDich, {
			liste: liste([eintrag({ stufe: 'jetzt' }), eintrag({ art: 'arbeit', id: 'ar-2' })])
		});
		expect(screen.queryByRole('region', { name: 'Jetzt für dich' })).toBeNull();
		expect(document.body.textContent.trim()).toBe('');
	});

	it('Etappe 3b-2: nur Fragen der Stufe Jetzt; Bald und Irgendwann stehen in „Anstehend“', () => {
		const bald = eintrag({
			art: 'frage',
			id: 'logo',
			titel: 'Welches Logo?',
			stufe: 'bald'
		});
		const irgendwann = eintrag({
			art: 'frage',
			id: 'notiz',
			projekt: 'notizen',
			projekt_titel: 'Notizen',
			titel: 'Notizen aufräumen?',
			stufe: 'irgendwann'
		});
		const { unmount } = render(JetztFuerDich, { liste: liste([bald, irgendwann]) });
		// Nur Fragen aus Bald und Irgendwann: Die Karte fehlt ganz.
		expect(screen.queryByRole('region', { name: 'Jetzt für dich' })).toBeNull();
		unmount();
		// Vom Inhaber auf Jetzt gestuft (wirksam Jetzt, Vorschlag Bald): Die Frage steht da.
		const gestuft = {
			...bald,
			stufe: { vorschlag: 'bald', inhaber: 'jetzt', wirksam: 'jetzt', grund: 'inhaber' }
		} satisfies typeof bald;
		render(JetztFuerDich, { liste: liste([gestuft, irgendwann]) });
		const k = karte();
		expect(within(k).getByRole('link', { name: 'Welches Logo?' })).toBeInTheDocument();
		expect(within(k).queryByText('Notizen aufräumen?')).toBeNull();
	});

	it('Ziel nicht mehr da: kein toter Link', () => {
		render(JetztFuerDich, {
			liste: liste([
				eintrag({ art: 'frage', id: 'weg', titel: 'Alte Frage', ziel_da: false, stufe: 'jetzt' })
			])
		});
		expect(screen.queryByRole('link', { name: 'Alte Frage' })).toBeNull();
		expect(screen.getByText('(nicht mehr da)')).toBeInTheDocument();
	});
});

describe('Jetzt für dich: Fristen im Archiv (Entscheidung archiv-status)', () => {
	const satz = 'Eine Frist in einem archivierten Projekt läuft am 08.11.2026 ab.';

	it('eine eigene Zeile ohne Projekttitel, mit Link zur Projektseite; auch ohne andere Einträge', () => {
		render(JetztFuerDich, {
			liste: {
				...liste([]),
				archiv_fristen: [
					{ satz, frist: '2026-11-08', tage_bis: 7, ziel: '/projects/stiller-umzug' }
				]
			}
		});
		const k = karte();
		const zeilen = within(k).getByRole('list', { name: 'Fristen im Archiv' });
		expect(zeilen).toHaveTextContent(satz);
		expect(within(zeilen).getByRole('link', { name: 'Zur Projektseite' })).toHaveAttribute(
			'href',
			'/projects/stiller-umzug'
		);
		expect(k.querySelectorAll('h3')).toHaveLength(0);
		expect(within(k).queryByText('Gerade wartet nichts weiter auf dich.')).toBeNull();
	});

	it('Auflage B3: Zeilen ohne Datum („hängt an“, „nicht lesbar“) nur in Anstehend, nicht hier', () => {
		const ohneDatum = [
			{
				satz: 'Eine Frist in einem archivierten Projekt hängt an einem Ereignis.',
				frist: null,
				tage_bis: null,
				ziel: '/projects/stiller-umzug'
			},
			{
				satz: 'Fristen eines archivierten Projekts lassen sich nicht lesen.',
				frist: null,
				tage_bis: null,
				ziel: '/projects/stiller-umzug'
			}
		];
		const r = render(JetztFuerDich, { liste: { ...liste([]), archiv_fristen: ohneDatum } });
		expect(screen.queryByRole('region', { name: 'Jetzt für dich' })).toBeNull();
		expect(screen.queryByText(/hängt an einem Ereignis/)).toBeNull();
		r.unmount();
		render(JetztFuerDich, {
			liste: {
				...liste([]),
				archiv_fristen: [
					{ satz, frist: '2026-11-08', tage_bis: 7, ziel: '/projects/stiller-umzug' },
					...ohneDatum
				]
			}
		});
		const zeilen = within(karte()).getByRole('list', { name: 'Fristen im Archiv' });
		expect(within(zeilen).getAllByRole('listitem')).toHaveLength(1);
		expect(zeilen).toHaveTextContent(satz);
	});

	it('ohne Fristen im Archiv und ohne Einträge fehlt die Karte', () => {
		render(JetztFuerDich, { liste: { ...liste([]), archiv_fristen: [] } });
		expect(screen.queryByRole('region', { name: 'Jetzt für dich' })).toBeNull();
	});
});
