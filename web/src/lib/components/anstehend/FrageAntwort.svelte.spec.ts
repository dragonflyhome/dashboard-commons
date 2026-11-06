import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import FrageAntwort from './FrageAntwort.svelte';
import AntwortStatus from './AntwortStatus.svelte';
import JetztFuerDich from './JetztFuerDich.svelte';
import { Antworten, ZEIGEN_MS } from '#lib/antworten.svelte.ts';
import {
	SATZ_FALSCH_GEKLICKT,
	SATZ_GESENDET,
	SATZ_NETZ,
	SATZ_NICHT_GESPEICHERT,
	SATZ_OHNE_ANMELDUNG,
	SATZ_SCHON_DA
} from '#lib/antworten.ts';
import { beispielListe, eintrag, json } from '#lib/anstehend.beispiel.ts';
import type { AnstehendEintrag, FrageAngabe } from '$vertrag';

function frage(angabe: FrageAngabe, titel = 'Welche Farbe?'): AnstehendEintrag {
	return eintrag({
		art: 'frage',
		id: 'farbe',
		projekt: 'probe',
		titel,
		gestellt: '2026-10-25',
		frage: angabe
	});
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

/** Der Bereich mit der Rückmeldung (aria-live, ohne eigene Rolle). */
function rueckmeldung(in_: ParentNode = document): HTMLElement {
	const el = in_.querySelector<HTMLElement>('.antwort-status');
	if (el === null) throw new Error('Rückmeldung fehlt');
	return el;
}

/** Ein festes Fokusziel außerhalb der Ansage, wie die Überschrift eines Abschnitts. */
function fokusZiel(): HTMLElement {
	const h = document.createElement('h2');
	h.tabIndex = -1;
	h.textContent = 'Abschnitt';
	document.body.append(h);
	return h;
}

function koerper(init: RequestInit | undefined): unknown {
	return JSON.parse(init?.body as string);
}

/** Frage und Rückmeldung zusammen, wie auf den Seiten. */
function zeige(e: AnstehendEintrag, speicher: Antworten, onbeantwortet = vi.fn()) {
	render(FrageAntwort, { eintrag: e, speicher, onbeantwortet });
	render(AntwortStatus, { speicher, ziel: null });
	return onbeantwortet;
}

describe('Frage beantworten', () => {
	it('Wahl: ein Knopf je Option, die Empfehlung ist vermerkt; Klick sendet und lädt neu', async () => {
		const { fetchFn, anfragen } = netz();
		const speicher = new Antworten(fetchFn);
		const neu = zeige(
			frage({ art: 'wahl', optionen: ['Blau', 'Grün'], empfehlung: 'Grün' }),
			speicher
		);
		const gruppe = screen.getByRole('group', { name: 'Antwort auf „Welche Farbe?“ (Auswahl)' });
		const knoepfe = within(gruppe).getAllByRole('button');
		expect(knoepfe.map((k) => k.textContent.trim())).toEqual([
			'Blau',
			'Grün (empfohlen)',
			'Etwas anderes …'
		]);
		await fireEvent.click(within(gruppe).getByRole('button', { name: 'Blau' }));
		await waitFor(() => {
			expect(rueckmeldung()).toHaveTextContent(SATZ_GESENDET);
		});
		expect(rueckmeldung()).toHaveTextContent(SATZ_FALSCH_GEKLICKT);
		expect(anfragen).toHaveLength(1);
		expect(anfragen[0]?.url).toBe('/api/antworten/probe/farbe');
		expect(anfragen[0]?.init?.method).toBe('POST');
		expect(anfragen[0]?.init?.headers).toMatchObject({ 'content-type': 'application/json' });
		expect(koerper(anfragen[0]?.init)).toEqual({ gestellt: '2026-10-25', antwort: 'Blau' });
		expect(neu).toHaveBeenCalledTimes(1);
	});

	it('Ja oder Nein: zwei Knöpfe, „Nein“ schickt „nein“', async () => {
		const { fetchFn, anfragen } = netz();
		const speicher = new Antworten(fetchFn);
		zeige(frage({ art: 'ja-nein', optionen: [], empfehlung: null }, 'Weitermachen?'), speicher);
		const gruppe = screen.getByRole('group', { name: /Weitermachen\?.*Ja oder Nein/ });
		expect(
			within(gruppe)
				.getAllByRole('button')
				.map((k) => k.textContent.trim())
		).toEqual(['Ja', 'Nein', 'Etwas anderes …']);
		await fireEvent.click(within(gruppe).getByRole('button', { name: 'Nein' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(1);
		});
		expect(koerper(anfragen[0]?.init)).toEqual({ gestellt: '2026-10-25', antwort: 'nein' });
	});

	it('Text: Feld mit Beschriftung, „Absenden“ erst mit Text, auch mit der Eingabetaste im Formular', async () => {
		const { fetchFn, anfragen } = netz();
		const speicher = new Antworten(fetchFn);
		zeige(frage({ art: 'text', optionen: [], empfehlung: null }, 'Was fehlt noch?'), speicher);
		const feld = screen.getByLabelText('Deine Antwort');
		expect(feld.tagName).toBe('TEXTAREA');
		expect(feld).toHaveAttribute('maxlength', '2000');
		const absenden = screen.getByRole('button', { name: 'Absenden' });
		expect(absenden).toBeDisabled();
		await fireEvent.input(feld, { target: { value: '   ' } });
		expect(absenden).toBeDisabled();
		await fireEvent.input(feld, { target: { value: 'Ein Logo.\nUnd ein Bild.' } });
		expect(absenden).toBeEnabled();
		await fireEvent.click(absenden);
		await waitFor(() => {
			expect(rueckmeldung()).toHaveTextContent(SATZ_GESENDET);
		});
		expect(koerper(anfragen[0]?.init)).toEqual({
			gestellt: '2026-10-25',
			antwort: 'Ein Logo.\nUnd ein Bild.'
		});
	});

	it('Bestätigen: ein Knopf, ohne Wert', async () => {
		const { fetchFn, anfragen } = netz(() =>
			json({ antwort: { antwort: 'bestaetigt' }, geaendert: false })
		);
		const speicher = new Antworten(fetchFn);
		zeige(frage({ art: 'bestaetigen', optionen: [], empfehlung: null }, 'Konzept so?'), speicher);
		await fireEvent.click(screen.getByRole('button', { name: 'Bestätigen' }));
		await waitFor(() => {
			expect(rueckmeldung()).toHaveTextContent(SATZ_SCHON_DA);
		});
		expect(koerper(anfragen[0]?.init)).toEqual({ gestellt: '2026-10-25' });
	});

	it('Fehler in einfachem Deutsch: Satz des Servers, ohne Anmeldung, ohne Netz; danach trotzdem neu laden', async () => {
		const satz = 'Diese Frage hast du schon beantwortet („Blau“).';
		const faelle: [() => Response, string][] = [
			[() => json({ fehler: satz, art: 'konflikt' }, 409), satz],
			[
				() => json({ fehler: 'Bitte mit dem Windows-Konto anmelden.', art: 'anmeldung' }, 401),
				SATZ_OHNE_ANMELDUNG
			],
			[
				() => {
					throw new TypeError('Failed to fetch');
				},
				SATZ_NETZ
			],
			[
				() => json({ fehler: 'Diese Frage ist nicht mehr offen.', art: 'nicht-gefunden' }, 404),
				'Diese Frage ist nicht mehr offen.'
			],
			// Ohne Datenspeicher oder bei einem Fehler im Server: nicht gespeichert.
			[
				() => json({ fehler: 'Der Datenspeicher fehlt.', art: 'speicher-fehlt' }, 503),
				SATZ_NICHT_GESPEICHERT
			],
			[() => new Response('Internal Server Error', { status: 500 }), SATZ_NICHT_GESPEICHERT],
			// Ohne JSON-Körper ein deutscher Satz, nie „413 Payload Too Large“.
			[
				() => new Response('', { status: 413, statusText: 'Payload Too Large' }),
				'Das ist zu lang zum Senden.'
			]
		];
		for (const [antwort, erwartet] of faelle) {
			const { fetchFn } = netz(antwort);
			const speicher = new Antworten(fetchFn);
			const neu = vi.fn();
			const { unmount } = render(FrageAntwort, {
				eintrag: frage({ art: 'ja-nein', optionen: [], empfehlung: null }),
				speicher,
				onbeantwortet: neu
			});
			const status = render(AntwortStatus, { speicher, ziel: null });
			await fireEvent.click(screen.getByRole('button', { name: 'Ja' }));
			await waitFor(() => {
				expect(rueckmeldung()).toHaveTextContent(erwartet);
			});
			expect(rueckmeldung().querySelector('p')).toHaveClass('block-error');
			expect(neu).toHaveBeenCalledTimes(1);
			unmount();
			status.unmount();
		}
	});

	it('während des Sendens sind die Knöpfe gesperrt', async () => {
		const halt: { fertig?: (r: Response) => void } = {};
		const fetchFn = vi.fn(
			() =>
				new Promise<Response>((r) => {
					halt.fertig = r;
				})
		) as unknown as typeof fetch;
		const speicher = new Antworten(fetchFn);
		render(FrageAntwort, {
			eintrag: frage({ art: 'ja-nein', optionen: [], empfehlung: null }),
			speicher
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Ja' }));
		expect(screen.getByRole('button', { name: 'Nein' })).toBeDisabled();
		expect(screen.getByRole('group', { name: /^Antwort auf/ })).toHaveAttribute(
			'aria-busy',
			'true'
		);
		expect(screen.getByText('Wird gesendet …')).toBeInTheDocument();
		halt.fertig?.(json({ antwort: {}, geaendert: true }));
		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Nein' })).toBeEnabled();
		});
		expect(fetchFn).toHaveBeenCalledTimes(1);
	});

	it('Anmerkung: zugeklappt, ohne Öffnen geht der Klick ohne; geöffnet und geschrieben geht sie mit', async () => {
		const { fetchFn, anfragen } = netz();
		const speicher = new Antworten(fetchFn);
		zeige(frage({ art: 'ja-nein', optionen: [], empfehlung: null }, 'Weitermachen?'), speicher);
		const zu = screen.getByText('Anmerkung (freiwillig)');
		expect(zu.tagName).toBe('SUMMARY');
		const details = zu.closest('details');
		expect(details).not.toHaveAttribute('open');
		const feld = screen.getByLabelText('Deine Anmerkung');
		expect(feld).toHaveAttribute('maxlength', '1000');
		expect(feld).toHaveAccessibleDescription(/Erst schreiben, dann oben auf deine Antwort klicken/);
		// Ohne Anmerkung: ein Klick, kein Feld notiz.
		await fireEvent.click(screen.getByRole('button', { name: 'Ja' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(1);
		});
		expect(koerper(anfragen[0]?.init)).toEqual({ gestellt: '2026-10-25', antwort: 'ja' });
		// Mit Anmerkung (auch mehrzeilig): geht mit dem nächsten Klick mit.
		await fireEvent.click(zu);
		await fireEvent.input(feld, { target: { value: 'Aber erst ab Montag.\nDanke' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Nein' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(2);
		});
		expect(koerper(anfragen[1]?.init)).toEqual({
			gestellt: '2026-10-25',
			antwort: 'nein',
			notiz: 'Aber erst ab Montag.\nDanke'
		});
	});

	it('Anmerkung auch bei Wahl und Bestätigen, nicht bei Text', async () => {
		const { fetchFn, anfragen } = netz();
		const speicher = new Antworten(fetchFn);
		const wahl = render(FrageAntwort, {
			eintrag: frage({ art: 'wahl', optionen: ['Blau', 'Grün'], empfehlung: null }),
			speicher
		});
		await fireEvent.input(screen.getByLabelText('Deine Anmerkung'), {
			target: { value: 'Eher hell.' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Grün' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(1);
		});
		expect(koerper(anfragen[0]?.init)).toMatchObject({ antwort: 'Grün', notiz: 'Eher hell.' });
		wahl.unmount();
		const best = render(FrageAntwort, {
			eintrag: frage({ art: 'bestaetigen', optionen: [], empfehlung: null }),
			speicher
		});
		await fireEvent.input(screen.getByLabelText('Deine Anmerkung'), {
			target: { value: 'Passt.' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Bestätigen' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(2);
		});
		expect(koerper(anfragen[1]?.init)).toEqual({ gestellt: '2026-10-25', notiz: 'Passt.' });
		best.unmount();
		render(FrageAntwort, {
			eintrag: frage({ art: 'text', optionen: [], empfehlung: null }),
			speicher
		});
		expect(screen.queryByText('Anmerkung (freiwillig)')).toBeNull();
		expect(screen.queryByLabelText('Deine Anmerkung')).toBeNull();
	});

	it('Vorlese-Programme: die Rückmeldung steht immer im Dokument; der Fokus geht auf das Ziel außerhalb', async () => {
		const { fetchFn } = netz();
		const speicher = new Antworten(fetchFn);
		const ziel = fokusZiel();
		const e = frage({ art: 'ja-nein', optionen: [], empfehlung: null });
		const frageTeil = render(FrageAntwort, { eintrag: e, speicher });
		render(AntwortStatus, { speicher, ziel });
		// Leer, aber da: sonst sagt das Vorlese-Programm die erste Meldung oft nicht an.
		const bereich = rueckmeldung();
		expect(bereich).toHaveAttribute('aria-live', 'polite');
		expect(bereich.textContent.trim()).toBe('');
		const knopf = screen.getByRole('button', { name: 'Ja' });
		knopf.focus();
		await fireEvent.click(knopf);
		await waitFor(() => {
			expect(bereich).toHaveTextContent(SATZ_GESENDET);
		});
		// Derselbe Bereich (nicht neu eingefügt); der Fokus liegt auf dem Ziel, nicht in der Ansage.
		expect(rueckmeldung()).toBe(bereich);
		await waitFor(() => {
			expect(document.activeElement).toBe(ziel);
		});
		expect(bereich.contains(document.activeElement)).toBe(false);
		expect(bereich.querySelector('[tabindex]')).toBeNull();
		// Die Frage verschwindet (Neuladen): der Fokus bleibt auf dem Ziel, nicht auf body.
		frageTeil.unmount();
		expect(document.activeElement).toBe(ziel);
		// Ein zweites Zeichnen derselben Meldung holt den Fokus nicht noch einmal.
		const anderer = document.createElement('button');
		document.body.append(anderer);
		anderer.focus();
		render(AntwortStatus, { speicher, ziel });
		expect(document.activeElement).toBe(anderer);
		anderer.remove();
		ziel.remove();
	});

	it('nach 15 s ist die Meldung weg, der Fokus bleibt auf dem Ziel', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		try {
			const { fetchFn } = netz();
			const speicher = new Antworten(fetchFn);
			const ziel = fokusZiel();
			render(FrageAntwort, {
				eintrag: frage({ art: 'ja-nein', optionen: [], empfehlung: null }),
				speicher
			});
			render(AntwortStatus, { speicher, ziel });
			await fireEvent.click(screen.getByRole('button', { name: 'Ja' }));
			await vi.waitFor(() => {
				expect(document.activeElement).toBe(ziel);
			});
			expect(rueckmeldung()).toHaveTextContent(SATZ_GESENDET);
			vi.advanceTimersByTime(ZEIGEN_MS);
			await tick();
			expect(rueckmeldung().textContent.trim()).toBe('');
			expect(document.activeElement).toBe(ziel);
			ziel.remove();
		} finally {
			vi.useRealTimers();
		}
	});

	it('nach einer gespeicherten Antwort sind Anmerkung und Text leer, nach einem Fehler nicht', async () => {
		let gut = true;
		const { fetchFn, anfragen } = netz(() =>
			gut ? json({ antwort: {}, geaendert: true }) : json({ fehler: 'kaputt' }, 500)
		);
		const speicher = new Antworten(fetchFn);
		const wahl = render(FrageAntwort, {
			eintrag: frage({ art: 'ja-nein', optionen: [], empfehlung: null }),
			speicher
		});
		const feld = screen.getByLabelText<HTMLTextAreaElement>('Deine Anmerkung');
		// Fehler: die Anmerkung bleibt für den nächsten Versuch stehen.
		gut = false;
		await fireEvent.input(feld, { target: { value: 'Erst ab Montag.' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Ja' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(1);
		});
		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Ja' })).toBeEnabled();
		});
		expect(feld.value).toBe('Erst ab Montag.');
		// Gespeichert: leer, der nächste Klick geht ohne Anmerkung.
		gut = true;
		await fireEvent.click(screen.getByRole('button', { name: 'Ja' }));
		await waitFor(() => {
			expect(feld.value).toBe('');
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Nein' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(3);
		});
		expect(koerper(anfragen[2]?.init)).toEqual({ gestellt: '2026-10-25', antwort: 'nein' });
		wahl.unmount();
		render(FrageAntwort, {
			eintrag: frage({ art: 'text', optionen: [], empfehlung: null }),
			speicher
		});
		const text = screen.getByLabelText<HTMLTextAreaElement>('Deine Antwort');
		await fireEvent.input(text, { target: { value: 'Blau' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Absenden' }));
		await waitFor(() => {
			expect(text.value).toBe('');
		});
	});

	it('ohne Angabe zur Frage zeigt der Baustein nichts', () => {
		render(FrageAntwort, { eintrag: eintrag({ art: 'frage', frage: null }) });
		expect(document.body.textContent.trim()).toBe('');
	});
});

describe('Jetzt für dich: Frage direkt beantworten', () => {
	/** Zwei Fragen eines Projekts, nacheinander gestellt. */
	function zweiFragen(art: FrageAngabe['art']): [AnstehendEintrag, AnstehendEintrag] {
		const angabe: FrageAngabe = { art, optionen: [], empfehlung: null };
		// Etappe 3b-2: „Jetzt für dich“ zeigt nur Fragen der Stufe Jetzt.
		const basis = { art: 'frage', projekt: 'probe', frage: angabe, stufe: 'jetzt' } as const;
		return [
			eintrag({ ...basis, id: 'erste', titel: 'Erste?', gestellt: '2026-10-25' }),
			eintrag({ ...basis, id: 'zweite', titel: 'Zweite?', gestellt: '2026-10-26' })
		];
	}

	it('die Frage hat Knöpfe; nach der letzten Antwort bleibt die Karte, der Fokus auf ihrer Überschrift', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		try {
			const { fetchFn } = netz();
			const speicher = new Antworten(fetchFn);
			const e = {
				...frage({ art: 'ja-nein', optionen: [], empfehlung: null }, 'Weitermachen?'),
				stufe: { vorschlag: 'jetzt', inhaber: null, wirksam: 'jetzt', grund: 'vorschlag' }
			} satisfies AnstehendEintrag;
			const onbeantwortet = vi.fn();
			const { rerender } = render(JetztFuerDich, {
				liste: { ...beispielListe(), eintraege: [e] },
				speicher,
				onbeantwortet
			});
			const karte = screen.getByRole('region', { name: 'Jetzt für dich' });
			const titel = within(karte).getByRole('heading', { name: 'Jetzt für dich' });
			expect(titel).toHaveAttribute('tabindex', '-1');
			await fireEvent.click(within(karte).getByRole('button', { name: 'Ja' }));
			await vi.waitFor(() => {
				expect(onbeantwortet).toHaveBeenCalledTimes(1);
			});
			// Neu geladen: nichts mehr offen. Die Karte zeigt die Rückmeldung.
			await rerender({ liste: { ...beispielListe(), eintraege: [] }, speicher, onbeantwortet });
			const danach = screen.getByRole('region', { name: 'Jetzt für dich' });
			expect(rueckmeldung(danach)).toHaveTextContent(SATZ_GESENDET);
			expect(within(danach).queryByRole('button')).toBeNull();
			// Der Fokus liegt auf der Überschrift: dieselbe, außerhalb der Ansage.
			expect(document.activeElement).toBe(titel);
			expect(rueckmeldung(danach).contains(titel)).toBe(false);
			// Nach 15 s ist die Meldung weg; Karte und Überschrift bleiben, der Fokus auch.
			vi.advanceTimersByTime(ZEIGEN_MS);
			await tick();
			expect(rueckmeldung(danach).textContent.trim()).toBe('');
			expect(screen.getByRole('region', { name: 'Jetzt für dich' })).toBe(danach);
			expect(within(danach).getByText('Gerade wartet nichts weiter auf dich.')).toBeInTheDocument();
			expect(document.activeElement).toBe(titel);
		} finally {
			vi.useRealTimers();
		}
	});

	it('eine Meldung von anderswo zeigt die Karte nur, solange sie steht', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		try {
			const { fetchFn } = netz();
			const speicher = new Antworten(fetchFn);
			await speicher.senden(frage({ art: 'bestaetigen', optionen: [], empfehlung: null }), null);
			render(JetztFuerDich, { liste: { ...beispielListe(), eintraege: [] }, speicher });
			expect(screen.getByRole('region', { name: 'Jetzt für dich' })).toBeInTheDocument();
			vi.advanceTimersByTime(ZEIGEN_MS);
			await tick();
			expect(screen.queryByRole('region', { name: 'Jetzt für dich' })).toBeNull();
		} finally {
			vi.useRealTimers();
		}
	});

	it('zwei Fragen eines Projekts: die zweite hat eine leere Anmerkung, nichts geht still mit', async () => {
		const { fetchFn, anfragen } = netz();
		const speicher = new Antworten(fetchFn);
		const [erste, zweite] = zweiFragen('ja-nein');
		const { rerender } = render(JetztFuerDich, {
			liste: { ...beispielListe(), eintraege: [erste] },
			speicher
		});
		await fireEvent.input(screen.getByLabelText('Deine Anmerkung'), {
			target: { value: 'Nur diese Woche.' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Ja' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(1);
		});
		expect(koerper(anfragen[0]?.init)).toMatchObject({ notiz: 'Nur diese Woche.' });
		// Neu geladen: die nächste Frage desselben Projekts rückt nach.
		await rerender({ liste: { ...beispielListe(), eintraege: [zweite] }, speicher });
		expect(screen.getByRole('group', { name: /^Antwort auf „Zweite\?“/ })).toBeInTheDocument();
		expect(screen.getByLabelText<HTMLTextAreaElement>('Deine Anmerkung').value).toBe('');
		await waitFor(() => {
			expect(screen.getByRole('button', { name: 'Nein' })).toBeEnabled();
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Nein' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(2);
		});
		expect(anfragen[1]?.url).toBe('/api/antworten/probe/zweite');
		expect(koerper(anfragen[1]?.init)).toEqual({ gestellt: '2026-10-26', antwort: 'nein' });
	});

	it('rückt die nächste Frage nach (etwa nach einem Fehler), erbt sie nichts von der vorigen', async () => {
		const speicher = new Antworten(netz().fetchFn);
		const [erste, zweite] = zweiFragen('ja-nein');
		const { rerender } = render(JetztFuerDich, {
			liste: { ...beispielListe(), eintraege: [erste] },
			speicher
		});
		await fireEvent.input(screen.getByLabelText('Deine Anmerkung'), {
			target: { value: 'Gehört zur ersten.' }
		});
		await rerender({ liste: { ...beispielListe(), eintraege: [zweite] }, speicher });
		expect(screen.getByLabelText<HTMLTextAreaElement>('Deine Anmerkung').value).toBe('');
		// Dieselbe id, neu gestellt: auch ein neuer Baustein.
		await fireEvent.input(screen.getByLabelText('Deine Anmerkung'), {
			target: { value: 'Alt.' }
		});
		await rerender({
			liste: { ...beispielListe(), eintraege: [{ ...zweite, gestellt: '2026-10-27' }] },
			speicher
		});
		expect(screen.getByLabelText<HTMLTextAreaElement>('Deine Anmerkung').value).toBe('');
	});

	it('zwei Text-Fragen eines Projekts: die zweite hat ein leeres Feld', async () => {
		const { fetchFn, anfragen } = netz();
		const speicher = new Antworten(fetchFn);
		const [erste, zweite] = zweiFragen('text');
		const { rerender } = render(JetztFuerDich, {
			liste: { ...beispielListe(), eintraege: [erste] },
			speicher
		});
		await fireEvent.input(screen.getByLabelText('Deine Antwort'), {
			target: { value: 'Blau' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Absenden' }));
		await waitFor(() => {
			expect(anfragen).toHaveLength(1);
		});
		await rerender({ liste: { ...beispielListe(), eintraege: [zweite] }, speicher });
		expect(screen.getByRole('group', { name: /^Antwort auf „Zweite\?“/ })).toBeInTheDocument();
		expect(screen.getByLabelText<HTMLTextAreaElement>('Deine Antwort').value).toBe('');
		expect(screen.getByRole('button', { name: 'Absenden' })).toBeDisabled();
	});
});

describe('Fokus nur aus dem eigenen Abschnitt (Auflage aus A2, Etappe A3)', () => {
	it('Fokus nicht verschoben: eine Antwort aus einem anderen Abschnitt holt den Fokus nicht', async () => {
		const { fetchFn } = netz();
		const speicher = new Antworten(fetchFn);
		const ziel = fokusZiel();
		render(FrageAntwort, {
			eintrag: frage({ art: 'ja-nein', optionen: [], empfehlung: null }),
			speicher,
			abschnitt: 'start'
		});
		render(AntwortStatus, { speicher, ziel, abschnitt: 'anstehend' });
		const knopf = screen.getByRole('button', { name: 'Ja' });
		knopf.focus();
		await fireEvent.click(knopf);
		await waitFor(() => {
			expect(rueckmeldung()).toHaveTextContent(SATZ_GESENDET);
		});
		await tick();
		expect(document.activeElement).not.toBe(ziel);
		expect(document.activeElement).toBe(knopf);
		ziel.remove();
	});

	it('Fokus nicht verschoben: wer während der Anfrage woanders hinklickt, behält seinen Fokus', async () => {
		let freigeben: () => void = () => undefined;
		const fetchFn = vi.fn(
			() =>
				new Promise<Response>((fertig) => {
					freigeben = () => {
						fertig(json({ antwort: {}, geaendert: true }));
					};
				})
		) as unknown as typeof fetch;
		const speicher = new Antworten(fetchFn);
		const ziel = fokusZiel();
		render(FrageAntwort, {
			eintrag: frage({ art: 'ja-nein', optionen: [], empfehlung: null }),
			speicher,
			abschnitt: 'anstehend'
		});
		render(AntwortStatus, { speicher, ziel, abschnitt: 'anstehend' });
		const knopf = screen.getByRole('button', { name: 'Ja' });
		knopf.focus();
		await fireEvent.click(knopf);
		const anderes = document.createElement('input');
		document.body.append(anderes);
		anderes.focus();
		freigeben();
		await waitFor(() => {
			expect(rueckmeldung()).toHaveTextContent(SATZ_GESENDET);
		});
		await tick();
		expect(document.activeElement).toBe(anderes);
		anderes.remove();
		ziel.remove();
	});

	it('gleicher Abschnitt, Fokus noch auf dem gesperrten Knopf: der Fokus geht auf das Ziel', async () => {
		const { fetchFn } = netz();
		const speicher = new Antworten(fetchFn);
		const ziel = fokusZiel();
		render(FrageAntwort, {
			eintrag: frage({ art: 'ja-nein', optionen: [], empfehlung: null }),
			speicher,
			abschnitt: 'projekt:probe'
		});
		render(AntwortStatus, { speicher, ziel, abschnitt: 'projekt:probe' });
		const knopf = screen.getByRole('button', { name: 'Ja' });
		knopf.focus();
		await fireEvent.click(knopf);
		await waitFor(() => {
			expect(document.activeElement).toBe(ziel);
		});
		ziel.remove();
	});

	it('gesperrt bleibt bis nach dem Neuladen, damit beim Nachrücken nichts verloren geht', async () => {
		const { fetchFn } = netz();
		const speicher = new Antworten(fetchFn);
		let neuGeladen: () => void = () => undefined;
		const danach = (): Promise<void> =>
			new Promise<void>((fertig) => {
				neuGeladen = fertig;
			});
		render(FrageAntwort, {
			eintrag: frage({ art: 'text', optionen: [], empfehlung: null }),
			speicher,
			onbeantwortet: danach
		});
		await fireEvent.input(screen.getByLabelText('Deine Antwort'), { target: { value: 'Blau' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Absenden' }));
		await waitFor(() => {
			expect(fetchFn).toHaveBeenCalledTimes(1);
		});
		await tick();
		// Die Antwort ist gespeichert, die Seite lädt noch: weiter gesperrt.
		expect(speicher.beschaeftigt).not.toBeNull();
		expect(screen.getByLabelText('Deine Antwort')).toBeDisabled();
		neuGeladen();
		await waitFor(() => {
			expect(speicher.beschaeftigt).toBeNull();
		});
	});
});
