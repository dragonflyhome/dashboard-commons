import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import AufgabenKasten from './AufgabenKasten.svelte';
import { SATZ_OHNE_ANMELDUNG } from '#lib/aufgaben.ts';
import type { AufgabeAnsicht } from '$vertrag';

/** Erfundener Testwert, kein echter Link. */
const LINK = 'https://anmeldung.beispiel.invalid/einmal?token=erfunden';

function aufgabe(teil: Partial<AufgabeAnsicht> = {}): AufgabeAnsicht {
	return {
		id: 'a-20261028-0001',
		projekt: 'dashboard',
		von: 'chat:dashboard',
		titel: 'Bei Probe-Dienst anmelden',
		knopf: null,
		frist: null,
		ablauf: null,
		zustand: 'offen',
		antwort: null,
		eigene: null,
		erstellt: '2026-10-28T08:00:00+01:00',
		geaendert: '2026-10-28T08:00:00+01:00',
		hat_link: false,
		vorgang: null,
		link: null,
		frist_stufe: null,
		...teil
	};
}

describe('AufgabenKasten', () => {
	it('zeigt Titel, Projekt, Chat, großen Link im neuen Tab und den Haken', async () => {
		const onhaken = vi.fn();
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [
				aufgabe({ link: LINK, hat_link: true, frist: '2026-10-28', frist_stufe: 'heute' })
			],
			fehler: null,
			onhaken,
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		// Etappe 3b-2: In Worten, was gezählt ist, nicht nur eine Zahl.
		expect(screen.getByRole('heading', { name: 'Für dich 1 offene Aufgabe' })).toBeInTheDocument();
		expect(screen.getByText('Bei Probe-Dienst anmelden')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'dashboard' })).toHaveAttribute(
			'href',
			'/projects/dashboard'
		);
		expect(screen.getByText(/Chat dashboard/)).toBeInTheDocument();
		const oeffnen = screen.getByRole('link', { name: /Öffnen/ });
		expect(oeffnen).toHaveAttribute('href', LINK);
		expect(oeffnen).toHaveAttribute('target', '_blank');
		expect(oeffnen).toHaveAttribute('rel', 'noopener noreferrer');
		// Das Ziel steht sichtbar auf dem Link.
		expect(oeffnen).toHaveTextContent('anmeldung.beispiel.invalid');
		expect(oeffnen.querySelector('.ziel')?.textContent).toBe('anmeldung.beispiel.invalid');
		// Frist heute (Stufe vom Server, Etappe 3a+): Achtung.
		expect(screen.getByText('heute fällig')).toBeInTheDocument();
		expect(screen.getByRole('listitem')).toHaveClass('achtung');
		// Der Haken ist ein echter Button.
		const haken = screen.getByRole('button', { name: 'Erledigt: Bei Probe-Dienst anmelden' });
		expect(haken.tagName).toBe('BUTTON');
		await fireEvent.click(haken);
		expect(onhaken).toHaveBeenCalledWith('a-20261028-0001', null);
	});

	it('Knöpfe statt Haken; Klick schickt die Antwort', async () => {
		const onhaken = vi.fn();
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [aufgabe({ knopf: ['ja', 'nein'], frist: '2026-11-03', frist_stufe: 'bald' })],
			fehler: null,
			onhaken,
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.queryByRole('button', { name: /^Erledigt/ })).toBeNull();
		expect(screen.queryByRole('link', { name: /Öffnen/ })).toBeNull();
		expect(screen.getByRole('listitem')).not.toHaveClass('achtung');
		expect(screen.getByText('bis 03.11.')).toBeInTheDocument();
		await fireEvent.click(screen.getByRole('button', { name: 'nein: Bei Probe-Dienst anmelden' }));
		expect(onhaken).toHaveBeenCalledWith('a-20261028-0001', 'nein');
	});

	it('überfällig ist Achtung; gesperrt, solange ein Haken läuft', () => {
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [aufgabe({ frist: '2026-10-20', frist_stufe: 'ueberfaellig' })],
			fehler: null,
			beschaeftigt: 'a-20261028-0001',
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.getByRole('listitem')).toHaveClass('achtung');
		expect(screen.getByText('überfällig seit 20.10.')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /^Erledigt/ })).toBeDisabled();
	});

	it('ohne Anmeldung nur der Satz für Edge, kein Fehler', () => {
		render(AufgabenKasten, {
			stand: 'ohne-anmeldung',
			aufgaben: [],
			fehler: SATZ_OHNE_ANMELDUNG,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		const link = screen.getByRole('link', { name: 'http://localhost:8787' });
		expect(link.closest('p')?.textContent).toBe(
			'Aufgaben siehst du in Microsoft Edge unter http://localhost:8787.'
		);
		expect(screen.queryByRole('alert')).toBeNull();
		expect(screen.queryByRole('button')).toBeNull();
	});

	it('nie ein javascript:-Link, auch wenn einer hereinkäme', () => {
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [aufgabe({ link: 'javascript:alert(1)', hat_link: true })],
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.queryByRole('link', { name: /Öffnen/ })).toBeNull();
		expect(screen.getByText('Der Link lässt sich nicht lesen.')).toBeInTheDocument();
	});

	it('Ziel in ASCII-Form; unsichtbare Zeichen oder zu großer Port: kein Link', () => {
		const rtl = String.fromCodePoint(0x202e);
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [
				aufgabe({ link: 'https://bücher.beispiel.invalid/a', hat_link: true }),
				aufgabe({
					id: 'a-20261028-0002',
					titel: 'Zwei',
					link: `https://beispiel.invalid/${rtl}fdp.exe`,
					hat_link: true
				}),
				aufgabe({
					id: 'a-20261028-0003',
					titel: 'Drei',
					link: 'https://beispiel.invalid:65536/',
					hat_link: true
				})
			],
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		const links = screen.getAllByRole('link', { name: /Öffnen/ });
		expect(links).toHaveLength(1);
		expect(links[0]?.querySelector('.ziel')?.textContent).toBe('xn--bcher-kva.beispiel.invalid');
		expect(screen.getAllByText('Der Link lässt sich nicht lesen.')).toHaveLength(2);
	});

	it('auf der Projektseite leer verborgen', () => {
		const { container } = render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [],
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null)),
			leerVerbergen: true
		});
		expect(container.querySelector('section')).toBeNull();
	});

	it('leer auf der Startseite: ein ruhiger Satz', () => {
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [],
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.getByText('Gerade keine offenen Aufgaben.')).toBeInTheDocument();
	});
	it('trägt je Aufgabe den Anker „aufgabe-<id>“ als Ziel (Etappe A1)', () => {
		render(AufgabenKasten, {
			stand: 'da',
			aufgaben: [aufgabe()],
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		const li = document.getElementById('aufgabe-a-20261028-0001');
		expect(li).toHaveTextContent('Bei Probe-Dienst anmelden');
		expect(li).toHaveAttribute('tabindex', '-1');
	});

	it('Etappe 3a+: Achtung nur nach der Frist-Stufe des Servers, nicht nach dem Datum im Browser', () => {
		render(AufgabenKasten, {
			stand: 'da',
			// Ein Datum weit in der Vergangenheit, aber der Server sagt „bald“: kein Achtung.
			aufgaben: [aufgabe({ frist: '2000-01-01', frist_stufe: 'bald' })],
			fehler: null,
			onhaken: vi.fn(),
			oneigene: vi.fn(() => Promise.resolve(null))
		});
		expect(screen.getByRole('listitem')).not.toHaveClass('achtung');
		expect(screen.getByText('bis 01.01.')).toBeInTheDocument();
	});
});
