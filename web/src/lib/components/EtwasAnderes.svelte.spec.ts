/**
 * Entscheidung „Etwas anderes …“: die Tests aus der Entscheidung
 * für die Oberfläche (Knopf bei jeder Klick-Antwort, Senden wie ein Klick,
 * leer und zu lang, Entwurf über Live-Änderung und Seitenwechsel, Tastatur).
 * Die Prüfung der Option in fragen.toml steht im Server (`pruefe`).
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import FrageAntwort from './anstehend/FrageAntwort.svelte';
import AntwortStatus from './anstehend/AntwortStatus.svelte';
import AufgabenKasten from './aufgaben/AufgabenKasten.svelte';
import EintragZeile from './anstehend/EintragZeile.svelte';
import { unterDashboardAnsicht, zustandWort } from '#lib/anstehend.ts';
import EtwasAnderes from './EtwasAnderes.svelte';
import { Antworten } from '#lib/antworten.svelte.ts';
import { SATZ_GESENDET } from '#lib/antworten.ts';
import { Aufgaben } from '#lib/aufgaben.svelte.ts';
import {
	EIGENE_HOECHSTENS,
	KNOPF_TEXT,
	SATZ_LEER,
	entwurfLesen,
	frageEntwurf
} from '#lib/etwas-anderes.ts';
import { eintrag, json } from '#lib/anstehend.beispiel.ts';
import type { AnstehendEintrag, AufgabeAnsicht, FrageAngabe } from '$vertrag';

function frage(angabe: FrageAngabe, titel = 'Welche Farbe?'): AnstehendEintrag {
	return eintrag({
		art: 'frage',
		id: 'farbe',
		projekt: 'probe',
		titel,
		gestellt: '2026-11-01',
		frage: angabe
	});
}

const WAHL: FrageAngabe = { art: 'wahl', optionen: ['Blau', 'Grün'], empfehlung: 'Grün' };
const JA_NEIN: FrageAngabe = { art: 'ja-nein', optionen: [], empfehlung: null };
const BESTAETIGEN: FrageAngabe = { art: 'bestaetigen', optionen: [], empfehlung: null };
const TEXT: FrageAngabe = { art: 'text', optionen: [], empfehlung: null };

function aufgabe(teil: Partial<AufgabeAnsicht> = {}): AufgabeAnsicht {
	return {
		id: 'a-20261101-0001',
		projekt: 'probe',
		von: 'chat:probe',
		titel: 'Abschicken?',
		knopf: ['ja', 'nein'],
		frist: null,
		ablauf: null,
		zustand: 'offen',
		antwort: null,
		eigene: null,
		erstellt: '2026-11-01T08:00:00+01:00',
		geaendert: '2026-11-01T08:00:00+01:00',
		hat_link: false,
		vorgang: null,
		link: null,
		frist_stufe: null,
		...teil
	};
}

/** fetch, das jede Anfrage merkt und mit `antwort` antwortet. */
function netz(antwort: () => Response = () => json({ antwort: {}, geaendert: true })) {
	const anfragen: { url: string; init: RequestInit | undefined }[] = [];
	const fetchFn = vi.fn((url: string, init?: RequestInit) => {
		anfragen.push({ url, init });
		return Promise.resolve(antwort());
	}) as unknown as typeof fetch;
	return { fetchFn, anfragen };
}

function koerper(init: RequestInit | undefined): unknown {
	return JSON.parse(init?.body as string);
}

function knopf(in_: HTMLElement = document.body): HTMLButtonElement {
	return within(in_).getByRole<HTMLButtonElement>('button', { name: /^Etwas anderes …/ });
}

function feld(): HTMLTextAreaElement {
	return screen.getByLabelText<HTMLTextAreaElement>('In eigenen Worten');
}

afterEach(() => {
	sessionStorage.clear();
});

describe('„Etwas anderes …“ bei jeder Klick-Antwort', () => {
	it('wahl, ja-nein und bestätigen zeigen den Knopf als letzten in der Reihe, text nicht', () => {
		for (const [angabe, letzteDavor] of [
			[WAHL, 'Grün (empfohlen)'],
			[JA_NEIN, 'Nein'],
			[BESTAETIGEN, '✓ Bestätigen']
		] as const) {
			const { unmount } = render(FrageAntwort, {
				eintrag: frage(angabe),
				speicher: new Antworten(netz().fetchFn)
			});
			const gruppe = screen.getByRole('group', { name: /Antwort auf „Welche Farbe\?“/ });
			const reihe = within(gruppe)
				.getAllByRole('button')
				.map((b) => b.textContent.trim());
			expect(reihe.at(-1), angabe.art).toBe(KNOPF_TEXT);
			expect(reihe.at(-2), angabe.art).toBe(letzteDavor);
			// Mit Bezug für Vorlese-Programme, zugeklappt.
			expect(knopf(gruppe)).toHaveAccessibleName(`${KNOPF_TEXT}: Welche Farbe?`);
			expect(knopf(gruppe)).toHaveAttribute('aria-expanded', 'false');
			unmount();
		}
		render(FrageAntwort, { eintrag: frage(TEXT), speicher: new Antworten(netz().fetchFn) });
		expect(screen.queryByRole('button', { name: /Etwas anderes/ })).toBeNull();
	});

	it('Aufgaben: nach den Knöpfen und nach dem Haken „erledigt“', () => {
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [aufgabe(), aufgabe({ id: 'a-20261101-0002', titel: 'Lesen', knopf: null })],
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		const [mitKnoepfen, mitHaken] = screen.getAllByRole('listitem');
		if (!mitKnoepfen || !mitHaken) throw new Error('zwei Aufgaben erwartet');
		const reihe = (li: HTMLElement) =>
			within(li)
				.getAllByRole('button')
				.map((b) => b.textContent.trim());
		expect(reihe(mitKnoepfen)).toEqual(['ja', 'nein', KNOPF_TEXT]);
		expect(reihe(mitHaken)).toEqual(['✓ erledigt', KNOPF_TEXT]);
		expect(knopf(mitHaken)).toHaveAccessibleName(`${KNOPF_TEXT}: Lesen`);
	});

	it('Bedienknöpfe des Dashboards bekommen ihn nicht: nur Klick-Antworten binden den Baustein ein', () => {
		const wurzel = join(import.meta.dirname, '..', '..');
		const alle = (pfad: string): string[] =>
			statSync(pfad).isDirectory() ? readdirSync(pfad).flatMap((n) => alle(join(pfad, n))) : [pfad];
		const nutzer = alle(wurzel)
			.filter((p) => p.endsWith('.svelte'))
			.filter((p) => readFileSync(p, 'utf8').includes('EtwasAnderes.svelte'))
			.map((p) => relative(wurzel, p).replaceAll('\\', '/'))
			.sort();
		expect(nutzer).toEqual([
			'lib/components/anstehend/EintragZeile.svelte',
			'lib/components/anstehend/FrageAntwort.svelte',
			'lib/components/aufgaben/AufgabenKasten.svelte',
			'routes/katalog/+page.svelte'
		]);
	});
});

describe('Senden wie ein Klick', () => {
	it('Frage: antwort = etwas-anderes mit eigene (und offener Anmerkung), Seite lädt neu, „Deine Antwort: …“', async () => {
		const { fetchFn, anfragen } = netz();
		const speicher = new Antworten(fetchFn);
		const neu = vi.fn();
		render(FrageAntwort, {
			eintrag: frage(JA_NEIN, 'Weitermachen?'),
			speicher,
			onbeantwortet: neu
		});
		render(AntwortStatus, { speicher, ziel: null });
		await fireEvent.input(screen.getByLabelText('Deine Anmerkung'), {
			target: { value: 'Ab dem 10.' }
		});
		await fireEvent.click(knopf());
		await fireEvent.input(feld(), { target: { value: '  Erst nach dem Urlaub,\nbitte.  ' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Senden' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(1);
		});
		expect(anfragen[0]?.url).toBe('/api/antworten/probe/farbe');
		expect(koerper(anfragen[0]?.init)).toEqual({
			gestellt: '2026-11-01',
			antwort: 'etwas-anderes',
			notiz: 'Ab dem 10.',
			eigene: 'Erst nach dem Urlaub,\nbitte.'
		});
		await waitFor(() => {
			expect(neu).toHaveBeenCalledTimes(1);
		});
		const status = document.querySelector<HTMLElement>('.antwort-status');
		expect(status).toHaveTextContent(SATZ_GESENDET);
		expect(status).toHaveTextContent('Deine Antwort: Erst nach dem Urlaub, bitte.');
		// Feld zu, Entwurf weg.
		await waitFor(() => {
			expect(screen.queryByLabelText('In eigenen Worten')).toBeNull();
		});
		expect(entwurfLesen(frageEntwurf(frage(JA_NEIN)))).toBe('');
	});

	it('Aufgabe: derselbe Weg wie der Haken, die Aufgabe bleibt offen, „Deine Antwort: …“', async () => {
		const offen = aufgabe({ antwort: 'etwas-anderes', eigene: 'Erst die Adresse prüfen.' });
		const anfragen: { url: string; init: RequestInit | undefined }[] = [];
		const fetchFn = vi.fn((url: string, init?: RequestInit) => {
			anfragen.push({ url, init });
			if (url.endsWith('/erledigt'))
				return Promise.resolve(json({ aufgabe: offen, geaendert: true }));
			if (url.endsWith('/anzahl')) return Promise.resolve(json({ offen: 1 }));
			return Promise.resolve(json({ aufgaben: [offen] }));
		}) as unknown as typeof fetch;
		const aufgaben = new Aufgaben(fetchFn);
		expect(await aufgaben.rueckmelden(offen.id, 'Erst die Adresse prüfen.')).toBeNull();
		const post = anfragen.find((a) => a.url.endsWith('/erledigt'));
		expect(post?.url).toBe('/api/aufgaben/a-20261101-0001/erledigt');
		expect(post?.init?.method).toBe('POST');
		expect(koerper(post?.init)).toEqual({
			antwort: 'etwas-anderes',
			eigene: 'Erst die Adresse prüfen.'
		});
		// Danach Liste und Zahl neu; die Aufgabe ist weiter offen und zeigt den Text.
		expect(aufgaben.liste.map((a) => a.zustand)).toEqual(['offen']);
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: aufgaben.liste,
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.getByText('Deine Antwort: Erst die Adresse prüfen.')).toBeInTheDocument();
	});

	it('Aufgabe im Kasten: oneigene bekommt Kennung und Text; Feld zu, Fokus zurück auf den Knopf', async () => {
		const oneigene = vi.fn(() => Promise.resolve(null));
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [aufgabe()],
			fehler: null,
			onhaken: vi.fn(),
			oneigene
		});
		await fireEvent.click(knopf());
		await fireEvent.input(feld(), { target: { value: 'Lieber morgen.' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Senden' }));
		await waitFor(() => {
			expect(oneigene).toHaveBeenCalledWith('a-20261101-0001', 'Lieber morgen.');
		});
		await waitFor(() => {
			expect(screen.queryByLabelText('In eigenen Worten')).toBeNull();
		});
		expect(document.activeElement).toBe(knopf());
	});
});

describe('Leer und zu lang', () => {
	it('leer oder nur Leerzeichen: ein Satz im Feld, nichts gesendet', async () => {
		const { fetchFn, anfragen } = netz();
		render(FrageAntwort, { eintrag: frage(WAHL), speicher: new Antworten(fetchFn) });
		await fireEvent.click(knopf());
		await fireEvent.input(feld(), { target: { value: '   \n ' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Senden' }));
		const satz = await screen.findByRole('alert');
		expect(satz).toHaveTextContent(SATZ_LEER);
		expect(feld()).toHaveAttribute('aria-invalid', 'true');
		expect(feld()).toHaveAccessibleDescription(expect.stringContaining(SATZ_LEER));
		expect(document.activeElement).toBe(feld());
		expect(anfragen).toHaveLength(0);
	});

	it('422 vom Server (etwa 20 001 Zeichen): der Satz steht im Feld, der Text bleibt', async () => {
		const SATZ = 'Deine Antwort in eigenen Worten („eigene“) ist zu lang: höchstens 20000 Zeichen.';
		const { fetchFn } = netz(() => json({ fehler: SATZ, art: 'unlesbar' }, 422));
		render(FrageAntwort, { eintrag: frage(WAHL), speicher: new Antworten(fetchFn) });
		await fireEvent.click(knopf());
		// Das Feld begrenzt technisch auf 20 000 Zeichen; mehr kommt nur ohne Browser an.
		expect(feld()).toHaveAttribute('maxlength', String(EIGENE_HOECHSTENS));
		const lang = 'x'.repeat(EIGENE_HOECHSTENS + 1);
		await fireEvent.input(feld(), { target: { value: lang } });
		await fireEvent.click(screen.getByRole('button', { name: 'Senden' }));
		await waitFor(() => {
			expect(screen.getByRole('alert')).toHaveTextContent(SATZ);
		});
		expect(feld().value).toBe(lang);
	});
});

describe('Der Entwurf geht nicht verloren', () => {
	it('überlebt eine Live-Änderung (neue Daten zur selben Frage) und einen Seitenwechsel', async () => {
		const speicher = new Antworten(netz().fetchFn);
		const { rerender, unmount } = render(FrageAntwort, { eintrag: frage(WAHL), speicher });
		await fireEvent.click(knopf());
		await fireEvent.input(feld(), { target: { value: 'Lieber Grau,\nwie das Logo.' } });
		// Live-Änderung: die Seite lädt die Liste neu, dieselbe Frage mit neuen Angaben.
		await rerender({ eintrag: { ...frage(WAHL), frist: '2026-11-05', tage_bis: 4 } });
		expect(feld().value).toBe('Lieber Grau,\nwie das Logo.');
		// Seitenwechsel: der Baustein verschwindet und kommt auf der nächsten Seite neu.
		unmount();
		expect(screen.queryByLabelText('In eigenen Worten')).toBeNull();
		// Auf der nächsten Seite (etwa „Jetzt für dich“ statt „Anstehend“) steht dieselbe Frage.
		render(FrageAntwort, { eintrag: frage(WAHL), speicher });
		expect(knopf()).toHaveAttribute('aria-expanded', 'true');
		expect(feld().value).toBe('Lieber Grau,\nwie das Logo.');
	});

	it('ohne sessionStorage läuft es trotzdem (nur ohne Merken)', async () => {
		const weg = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
			throw new Error('gesperrt');
		});
		render(EtwasAnderes, {
			entwurf: 'probe',
			bezug: 'Probe',
			senden: () => Promise.resolve(null)
		});
		await fireEvent.click(knopf());
		await fireEvent.input(feld(), { target: { value: 'Text' } });
		expect(feld().value).toBe('Text');
		weg.mockRestore();
	});
});

describe('Tastatur und Vorlese-Programm', () => {
	it('Knopf ist ein echter Knopf in der Tab-Reihenfolge; Klick klappt auf, Fokus ins Feld', async () => {
		render(FrageAntwort, { eintrag: frage(WAHL), speicher: new Antworten(netz().fetchFn) });
		const k = knopf();
		expect(k.tagName).toBe('BUTTON');
		expect(k).not.toHaveAttribute('tabindex');
		k.focus();
		expect(document.activeElement).toBe(k);
		await fireEvent.click(k);
		await tick();
		expect(k).toHaveAttribute('aria-expanded', 'true');
		const f = feld();
		expect(k.getAttribute('aria-controls')).toBe(f.form?.id);
		expect(document.activeElement).toBe(f);
		// Das Feld ist beschriftet und hat einen Hinweis; kein Fenster über der Seite.
		expect(f).toHaveAccessibleDescription(expect.stringContaining('Esc schließt das Feld'));
		expect(screen.queryByRole('dialog')).toBeNull();
		expect(screen.getByRole('form', { name: 'Etwas anderes: Welche Farbe?' })).toContainElement(f);
	});

	it('Esc schließt nur das leere Feld; der Fokus kehrt auf den Knopf zurück', async () => {
		render(FrageAntwort, { eintrag: frage(WAHL), speicher: new Antworten(netz().fetchFn) });
		await fireEvent.click(knopf());
		await fireEvent.input(feld(), { target: { value: 'Schon etwas' } });
		await fireEvent.keyDown(feld(), { key: 'Escape' });
		await tick();
		expect(feld().value).toBe('Schon etwas');
		await fireEvent.input(feld(), { target: { value: '  ' } });
		await fireEvent.keyDown(feld(), { key: 'Escape' });
		await waitFor(() => {
			expect(screen.queryByLabelText('In eigenen Worten')).toBeNull();
		});
		expect(knopf()).toHaveAttribute('aria-expanded', 'false');
		expect(document.activeElement).toBe(knopf());
	});

	it('„Abbrechen“ schließt, behält den Text und gibt den Fokus an den Knopf; aufklappen zeigt ihn wieder', async () => {
		render(FrageAntwort, { eintrag: frage(WAHL), speicher: new Antworten(netz().fetchFn) });
		await fireEvent.click(knopf());
		await fireEvent.input(feld(), { target: { value: 'Doch nicht' } });
		expect(entwurfLesen(frageEntwurf(frage(WAHL)))).toBe('Doch nicht');
		await fireEvent.click(screen.getByRole('button', { name: 'Abbrechen' }));
		await waitFor(() => {
			expect(document.activeElement).toBe(knopf());
		});
		expect(screen.queryByLabelText('In eigenen Worten')).toBeNull();
		expect(entwurfLesen(frageEntwurf(frage(WAHL)))).toBe('Doch nicht');
		await fireEvent.click(knopf());
		await tick();
		expect(feld().value).toBe('Doch nicht');
		expect(document.activeElement).toBe(feld());
	});

	it('„Text verwerfen“ gibt es nur mit Text; es leert Feld und Entwurf, das Feld bleibt offen', async () => {
		render(FrageAntwort, { eintrag: frage(WAHL), speicher: new Antworten(netz().fetchFn) });
		await fireEvent.click(knopf());
		expect(screen.queryByRole('button', { name: 'Text verwerfen' })).toBeNull();
		await fireEvent.input(feld(), { target: { value: 'Doch nicht' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Text verwerfen' }));
		await waitFor(() => {
			expect(document.activeElement).toBe(feld());
		});
		expect(feld().value).toBe('');
		expect(entwurfLesen(frageEntwurf(frage(WAHL)))).toBe('');
		expect(screen.queryByRole('button', { name: 'Text verwerfen' })).toBeNull();
	});

	// Für das Neuladen von selbst (fassung.ts): ein gesicherter Entwurf ist kein Sperrgrund.
	it('data-entwurf-gesichert nur, wenn der Entwurf wirklich in sessionStorage steht', async () => {
		render(FrageAntwort, { eintrag: frage(WAHL), speicher: new Antworten(netz().fetchFn) });
		await fireEvent.click(knopf());
		await fireEvent.input(feld(), { target: { value: 'Doch nicht' } });
		await tick();
		expect(feld()).toHaveAttribute('data-entwurf-gesichert');
		const setzen = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
			throw new Error('Speicher gesperrt');
		});
		try {
			await fireEvent.input(feld(), { target: { value: 'Doch nicht ganz' } });
			await tick();
			expect(feld()).not.toHaveAttribute('data-entwurf-gesichert');
		} finally {
			setzen.mockRestore();
		}
	});
});

describe('Aufgabe mit eigenen Worten liegt „beim Chat“', () => {
	function anstehendAufgabe(teil: Partial<Omit<AnstehendEintrag, 'stufe'>> = {}): AnstehendEintrag {
		return eintrag({
			art: 'aufgabe',
			id: 'a-20261101-0001',
			projekt: 'probe',
			titel: 'Abschicken?',
			knopf: ['ja', 'nein'],
			...teil
		});
	}
	const beimChat = (): AnstehendEintrag =>
		anstehendAufgabe({
			zustand: 'beim-chat',
			wartet_auf: 'chat:probe',
			wartet_auf_dich: false,
			eigene: 'Erst die Adresse prüfen.'
		});

	it('Kasten „Für dich“: nicht mitgezählt, ohne Knöpfe unter „Beim Chat“ mit deinem Text', () => {
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [
				aufgabe({ antwort: 'etwas-anderes', eigene: 'Erst die Adresse prüfen.' }),
				aufgabe({ id: 'a-20261101-0002', titel: 'Lesen', knopf: null })
			],
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('1 offene Aufgabe');
		const kopf = screen.getByRole('heading', { name: 'Beim Chat' });
		const liste = kopf.parentElement?.querySelector('li.beim-chat');
		if (!liste) throw new Error('Liste „Beim Chat“ fehlt');
		expect(liste).toHaveTextContent('Abschicken?');
		expect(liste).toHaveTextContent('beim Chat');
		expect(liste).toHaveTextContent('Deine Antwort: Erst die Adresse prüfen.');
		expect(within(liste as HTMLElement).queryAllByRole('button')).toHaveLength(0);
		expect(screen.queryByRole('button', { name: 'ja: Abschicken?' })).toBeNull();
		// Die andere Aufgabe wartet weiter auf dich, mit Haken und „Etwas anderes …“.
		expect(screen.getByRole('button', { name: 'Erledigt: Lesen' })).toBeInTheDocument();
	});

	it('Anstehend: „beim Chat“ mit deinem Text, ohne Knöpfe; eine offene Aufgabe hat „Etwas anderes …“', () => {
		const props = { gesperrt: false, onumstufen: vi.fn(), onhaken: vi.fn() };
		const { unmount } = render(EintragZeile, {
			eintrag: beimChat(),
			...props,
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.getByText(/beim Chat/)).toBeInTheDocument();
		expect(screen.getByText('Deine Antwort: Erst die Adresse prüfen.')).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'ja: Abschicken?' })).toBeNull();
		expect(screen.queryByRole('button', { name: /Etwas anderes/ })).toBeNull();
		expect(zustandWort(beimChat())).toBe('beim Chat');
		unmount();
		const oneigene = vi.fn(() => Promise.resolve(null));
		render(EintragZeile, { eintrag: anstehendAufgabe(), ...props, oneigene });
		const reihe = screen
			.getAllByRole('button')
			.map((b) => b.textContent.trim())
			.slice(-3);
		expect(reihe).toEqual(['ja', 'nein', KNOPF_TEXT]);
	});

	it('Unter-Dashboard: „Wartet auf dich“ ohne die Aufgabe beim Chat', () => {
		const offen = anstehendAufgabe({
			id: 'a-20261101-0002',
			titel: 'Lesen',
			wartet_auf_dich: true
		});
		const a = unterDashboardAnsicht(null, [beimChat(), offen], []);
		expect(a.wartet.aufgaben.map((e) => e.id)).toEqual(['a-20261101-0002']);
		expect(a.wartet.aufgabenZahl).toBe(1);
		expect(a.wartet.zahl).toBe(1);
	});
});
