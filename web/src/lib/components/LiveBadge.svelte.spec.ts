import { fireEvent, render, screen } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { LiveZustand } from '#lib/verbindung.ts';

// Am Live-Punkt steht höchstens einer von
// drei Zuständen (Vorrang 1 vor 3 vor 2), als Information. Nur Zustand 2 ist
// ein Knopf (Abkürzung, die Seite lädt ohnehin von selbst); jeder Wechsel
// wird einmal angesagt.
vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
vi.mock('$app/env', () => ({ version: '20261104-120000' }));

const { LAGE_TEXT, TEXT_AUFGEHALTEN, umgebung } = await import('#lib/fassung.ts');
const { fassung } = await import('#lib/fassung.svelte.ts');
const { bauLage } = await import('#lib/bau.svelte.ts');
const { apiStand } = await import('#lib/api/antwortet.svelte.ts');
const { freierKanal } = await import('#lib/freikanal.svelte.ts');
const { live } = await import('#lib/live.svelte.ts');
const { default: LiveBadge } = await import('./LiveBadge.svelte');

const KEINE = LAGE_TEXT.keine_verbindung;
const BAUT = LAGE_TEXT.baut;
const VERALTET = LAGE_TEXT.veraltet;

const ursprung = { ...umgebung };
afterEach(() => {
	Object.assign(umgebung, ursprung);
	fassung.veraltet = false;
	fassung.aufgehalten = false;
	bauLage.liste([]);
	apiStand.antwortet = true;
	freierKanal.aktiv = null;
	live.connected = false;
	document.body.innerHTML = '';
});

function ansage(): string {
	return document.querySelector('[role="status"]')?.textContent ?? '';
}

describe('Live-Punkt: drei Zustände', () => {
	it('kein Zustand: nur der Link zum Stand, kein Knopf, keine Ansage', () => {
		render(LiveBadge, { zustand: 'angemeldet' });
		expect(screen.getByRole('link')).toHaveTextContent('live, angemeldet');
		expect(screen.queryByRole('button')).toBeNull();
		expect(ansage()).toBe('');
	});

	it('2 App veraltet: ein Knopf (class button) mit genau diesem Text, einmal angesagt', () => {
		render(LiveBadge, { zustand: 'angemeldet', veraltet: true });
		const knopf = screen.getByRole('button');
		expect(knopf).toHaveAccessibleName(VERALTET);
		expect(knopf.textContent.trim()).toBe(VERALTET);
		expect(knopf).toHaveAttribute('type', 'button');
		expect(knopf).toHaveClass('button');
		expect(knopf).toHaveAttribute('data-lage', 'veraltet');
		expect(screen.queryByRole('link')).toBeNull();
		expect(ansage()).toBe(VERALTET);
		// Information, keine Aufforderung: kein „bitte“, kein „Klicken Sie“, kein Countdown.
		const alles = `${knopf.textContent} ${knopf.title} ${ansage()}`;
		expect(alles).not.toMatch(/bitte|klicken|muss|\d/i);
	});

	it('2 mit Sperrgrund: „App wird aktualisiert, sobald du fertig bist“', () => {
		render(LiveBadge, { zustand: 'angemeldet', veraltet: true, aufgehalten: true });
		expect(screen.getByRole('button')).toHaveAccessibleName(TEXT_AUFGEHALTEN);
	});

	it('3 Bau läuft: Text am Link, kein Knopf, nichts anzuklicken außer dem Link', () => {
		render(LiveBadge, { zustand: 'angemeldet', baut: true, veraltet: true });
		const link = screen.getByRole('link');
		expect(link).toHaveTextContent(BAUT);
		expect(link).toHaveAccessibleName(`${BAUT}, Stand ansehen`);
		expect(link).toHaveAttribute('data-lage', 'baut');
		expect(screen.queryByRole('button')).toBeNull();
		expect(ansage()).toBe(BAUT);
	});

	it.each(['unterbrochen', 'nicht_gefunden'] as LiveZustand[])(
		'1 keine Verbindung (%s): geht vor 3 und 2; Link bleibt Link',
		(zustand) => {
			const { unmount } = render(LiveBadge, { zustand, baut: true, veraltet: true });
			const link = screen.getByRole('link');
			expect(link).toHaveTextContent(KEINE);
			expect(link).toHaveAccessibleName(`${KEINE}, Stand ansehen`);
			expect(link).toHaveAttribute('data-zustand', zustand);
			// Ohne Warnfarbe: der Punkt grau (Klasse wie beim ersten Versuch).
			expect(link).toHaveClass('sucht');
			expect(link).not.toHaveClass('wartet');
			expect(link).not.toHaveClass('problem');
			expect(screen.queryByRole('button')).toBeNull();
			expect(ansage()).toBe(KEINE);
			unmount();
		}
	);

	it('1 auch, wenn der Kanal steht, die API aber nicht antwortet', () => {
		apiStand.antwortet = false;
		render(LiveBadge, { zustand: 'angemeldet', veraltet: true });
		const link = screen.getByRole('link');
		expect(link).toHaveTextContent(KEINE);
		expect(link).toHaveAttribute('data-lage', 'keine_verbindung');
		expect(link).toHaveClass('sucht');
		expect(screen.queryByRole('button')).toBeNull();
	});

	it('jeder Wechsel einmal angesagt, weitere Ereignisse derselben Lage nicht', async () => {
		const { rerender } = render(LiveBadge, { zustand: 'angemeldet', baut: true });
		const region = document.querySelector('[role="status"]');
		expect(ansage()).toBe(BAUT);
		await rerender({ zustand: 'angemeldet', baut: true, veraltet: true });
		expect(ansage()).toBe(BAUT);
		await rerender({ zustand: 'angemeldet', baut: false, veraltet: true });
		expect(ansage()).toBe(VERALTET);
		// Umschalten auf „sobald du fertig bist“: einmal angesagt (Sollte 7).
		await rerender({ zustand: 'angemeldet', veraltet: true, aufgehalten: true });
		expect(ansage()).toBe(TEXT_AUFGEHALTEN);
		await rerender({ zustand: 'unterbrochen', veraltet: true, aufgehalten: false });
		expect(ansage()).toBe(KEINE);
		await rerender({ zustand: 'angemeldet', veraltet: true });
		expect(ansage()).toBe(VERALTET);
		expect(document.querySelector('[role="status"]')).toBe(region);
	});

	it('aufgehalten: das Umschalten (false -> true) einmal angesagt, das Zurückschalten nicht', async () => {
		const { rerender } = render(LiveBadge, { zustand: 'angemeldet', veraltet: true });
		expect(ansage()).toBe(VERALTET);
		await rerender({ zustand: 'angemeldet', veraltet: true, aufgehalten: true });
		expect(ansage()).toBe(TEXT_AUFGEHALTEN);
		expect(screen.getByRole('button')).toHaveAccessibleName(TEXT_AUFGEHALTEN);
		await rerender({ zustand: 'angemeldet', veraltet: true, aufgehalten: false });
		// Zurück: der Knopf sagt wieder „gleich“, die Region wird still leer (kein neuer Satz).
		expect(screen.getByRole('button')).toHaveAccessibleName(VERALTET);
		expect(ansage()).toBe('');
		// Ein zweites Umschalten in derselben Lage wird wieder angesagt.
		await rerender({ zustand: 'angemeldet', veraltet: true, aufgehalten: true });
		expect(ansage()).toBe(TEXT_AUFGEHALTEN);
	});

	it('ohne Zustand wird die Region leer (nach dem Neuladen keine Ansage)', async () => {
		const { rerender } = render(LiveBadge, { zustand: 'unterbrochen' });
		expect(ansage()).toBe(KEINE);
		await rerender({ zustand: 'angemeldet' });
		expect(ansage()).toBe('');
	});

	it('Klick auf den Knopf lädt die Seite sofort neu (Abkürzung)', async () => {
		const neuLaden = vi.fn<() => void>();
		umgebung.neuLaden = neuLaden;
		render(LiveBadge, { zustand: 'angemeldet', veraltet: true });
		await fireEvent.click(screen.getByRole('button', { name: VERALTET }));
		expect(neuLaden).toHaveBeenCalledTimes(1);
	});

	it('Klick bei ungesichertem Text (Antwort, Fokus wandert beim Klick): kein Neuladen, aufgehalten', async () => {
		const neuLaden = vi.fn<() => void>();
		umgebung.neuLaden = neuLaden;
		const feld = document.createElement('textarea');
		feld.value = 'Halb geschrieben';
		document.body.append(feld);
		render(LiveBadge, { zustand: 'angemeldet', veraltet: true });
		const knopf = screen.getByRole('button', { name: VERALTET });
		knopf.focus();
		await fireEvent.click(knopf);
		expect(neuLaden).not.toHaveBeenCalled();
		expect(fassung.aufgehalten).toBe(true);
		expect(feld).toHaveFocus();
		expect(feld.value).toBe('Halb geschrieben');
	});

	it.each(['nicht_angemeldet', 'nicht_registriert'] as const)(
		'ohne Anmeldung (%s): dieselben Texte, ohne Satz aus dem Bau und ohne Ziffer',
		(zustand) => {
			bauLage.ereignis({
				slug: 'probe',
				zustand: 'laeuft',
				meldung: 'Satz aus dem Bau',
				gescheitert: false
			});
			const { unmount } = render(LiveBadge, { zustand, veraltet: true });
			const knopf = screen.getByRole('button', { name: VERALTET });
			expect(knopf.textContent).not.toMatch(/\d|Satz aus dem Bau/);
			unmount();
			const zwei = render(LiveBadge, { zustand, baut: true });
			expect(screen.getByRole('link')).toHaveTextContent(BAUT);
			expect(screen.getByRole('link').textContent).not.toMatch(/\d|Satz aus dem Bau/);
			zwei.unmount();
		}
	);

	it('ohne Anmeldung zählt nur die Projektliste (bau.zustand), kein Ereignis bau; angemeldet das Ereignis', () => {
		bauLage.ereignis({ slug: 'probe', zustand: 'laeuft', meldung: '', gescheitert: false });
		const frei = render(LiveBadge, { zustand: 'nicht_angemeldet', veraltet: true });
		expect(screen.getByRole('button')).toHaveAccessibleName(VERALTET);
		frei.unmount();
		bauLage.liste([
			{ slug: 'probe', bau: { zustand: 'laeuft', letzte_gute: null, gebaut_um: null } }
		] as unknown as Parameters<typeof bauLage.liste>[0]);
		const freiBau = render(LiveBadge, { zustand: 'nicht_angemeldet', veraltet: true });
		expect(screen.getByRole('link')).toHaveTextContent(BAUT);
		expect(screen.getByRole('link').textContent).not.toMatch(/probe|\d/);
		freiBau.unmount();
		bauLage.liste([]);
		bauLage.ereignis({ slug: 'probe', zustand: 'laeuft', meldung: '', gescheitert: false });
		freierKanal.aktiv = 'geschuetzt';
		live.connected = true;
		render(LiveBadge, { zustand: 'angemeldet', veraltet: true });
		expect(screen.getByRole('link')).toHaveTextContent(BAUT);
		expect(screen.queryByRole('button')).toBeNull();
	});

	it('Fokus wandert mit: vom Link auf den Knopf und zurück', async () => {
		const { rerender } = render(LiveBadge, { zustand: 'angemeldet' });
		screen.getByRole('link').focus();
		expect(screen.getByRole('link')).toHaveFocus();
		fassung.ereignis('20261104-120500');
		flushSync();
		expect(screen.getByRole('button', { name: VERALTET })).toHaveFocus();
		await rerender({ zustand: 'unterbrochen' });
		expect(screen.getByRole('link')).toHaveFocus();
	});

	it('Fokus anderswo: der Wechsel nimmt ihn nicht weg', () => {
		const feld = document.createElement('input');
		document.body.append(feld);
		render(LiveBadge, { zustand: 'angemeldet' });
		feld.focus();
		fassung.ereignis('20261104-120500');
		flushSync();
		expect(feld).toHaveFocus();
	});

	it('solange der erste Versuch läuft: kein Zustand', () => {
		render(LiveBadge, { zustand: null, veraltet: true, baut: true });
		expect(screen.queryByRole('button')).toBeNull();
		expect(ansage()).toBe('');
	});
});
