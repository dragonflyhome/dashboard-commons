import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { kopfleiste } from '#lib/kopfleiste.ts';

vi.mock('$app/paths', () => ({
	resolve: (p: string, params: Record<string, string> = {}) =>
		p.replace(/\[(\w+)\]/g, (_: string, k: string) => params[k] ?? k),
	asset: (p: string) => `/${p}`
}));
const { default: Brotkrumen } = await import('./Brotkrumen.svelte');

const neu = {
	kopf: {
		kopf: {
			title: 'Architektur des Dashboards',
			seiten: [{ name: 'zeitachse', title: 'Neue Zeitachse', order: 1 }]
		}
	}
};

describe('Brotkrumen (Kopfleiste, Fassung 3)', () => {
	it('Symbol › Projekte › Projekt › Blatt; das Blatt ist ein Knopf und die aktuelle Seite', () => {
		const kopf = kopfleiste('/projects/[slug]/[name]', { slug: 'bauplan', name: 'zeitachse' }, neu);
		render(Brotkrumen, { kopf, onblatt: () => Promise.resolve() });
		const nav = screen.getByRole('navigation', { name: 'Brotkrumen' });
		const punkte = within(nav).getAllByRole('listitem');
		expect(punkte.map((p) => p.textContent.trim())).toEqual([
			'',
			'…Projekte › Architektur des Dashboards',
			'Projekte',
			'Architektur des Dashboards',
			// Dazu die unsichtbare Beschreibung des Blatts für Vorlese-Programme.
			'Neue Zeitachse Seite auf den neuesten Stand bringen und nach oben'
		]);
		expect(within(nav).getByRole('link', { name: 'Startseite' })).toHaveAttribute('href', '/');
		const blatt = within(nav).getByRole('button', { name: /^Neue Zeitachse/ });
		expect(blatt).toHaveAttribute('aria-current', 'page');
		expect(nav).toHaveAttribute('title', 'Projekte · Architektur des Dashboards · Neue Zeitachse');
	});

	it('am Handy: mittlere Glieder als „…“, für Vorlese-Programme steht der Weg darin', () => {
		const kopf = kopfleiste(
			'/projects/[slug]',
			{ slug: 'schaufenster' },
			{ title: 'Schaufenster' }
		);
		render(Brotkrumen, { kopf, onblatt: () => Promise.resolve() });
		const auslassung = document.querySelector('.auslassung');
		// Sichtbar nur „…“, für Vorlese-Programme der ganze Weg.
		expect(auslassung?.querySelector('[aria-hidden="true"]')).toHaveTextContent('…');
		expect(auslassung?.querySelector('.nur-vorlesen')).toHaveTextContent('Projekte');
		// Die Glieder sind da (die Regel für 37,5 rem blendet sie nur sichtbar aus).
		expect(screen.getByRole('link', { name: 'Projekte' })).toBeInTheDocument();
	});

	it('Startseite: nur Symbol mit Ring und Blatt, kein „…“', () => {
		render(Brotkrumen, { kopf: kopfleiste('/', {}, {}), onblatt: () => Promise.resolve() });
		expect(screen.getByRole('link', { name: 'Startseite' })).toHaveClass('hier');
		expect(document.querySelector('.auslassung')).toBeNull();
		expect(screen.getByRole('button', { name: /^Übersicht/ })).toBeInTheDocument();
	});

	it('Klick auf das Blatt: nach oben (ohne Animation bei reduzierter Bewegung) und abgleichen', async () => {
		const nach = vi.spyOn(window, 'scrollTo').mockImplementation(() => undefined);
		const onblatt = vi.fn(() => Promise.resolve());
		render(Brotkrumen, { kopf: kopfleiste('/daten', {}, {}), onblatt });
		await fireEvent.click(screen.getByRole('button', { name: /^Daten/ }));
		expect(nach).toHaveBeenCalledWith({ top: 0, behavior: 'smooth' });
		expect(onblatt).toHaveBeenCalledTimes(1);
		nach.mockRestore();
	});

	it('das Blatt hat eine Beschreibung für Vorlese-Programme (unsichtbar)', () => {
		render(Brotkrumen, { kopf: kopfleiste('/daten', {}, {}), onblatt: () => Promise.resolve() });
		const blatt = screen.getByRole('button', { name: /^Daten/ });
		expect(blatt).toHaveAccessibleDescription('Seite auf den neuesten Stand bringen und nach oben');
		const id = blatt.getAttribute('aria-describedby') ?? '';
		expect(document.getElementById(id)).toHaveClass('nur-vorlesen');
	});

	it('Ergebnis: Häkchen und höfliche Ansage', () => {
		render(Brotkrumen, {
			kopf: kopfleiste('/daten', {}, {}),
			ergebnis: 'aktuell',
			onblatt: () => Promise.resolve()
		});
		expect(screen.getByRole('button', { name: /^Daten/ })).toHaveClass('geprueft');
		expect(document.querySelector('[aria-live="polite"]')).toHaveTextContent('Alles war aktuell.');
	});

	describe('lange Projekttitel verbreitern die Seite nicht (360 px)', () => {
		const quelle = readFileSync(join(import.meta.dirname, 'Brotkrumen.svelte'), 'utf8').replace(
			/\r\n/g,
			'\n'
		);
		/** Der Block einer CSS-Regel aus dem Stil der Komponente. */
		const regel = (selektor: string): string => {
			const i = quelle.indexOf(`\n\t${selektor} {`);
			expect(i, selektor).toBeGreaterThan(-1);
			return quelle.slice(i, quelle.indexOf('}', i));
		};
		const OHNE_LEERZEICHEN = 'Versuchsanlage'.repeat(6).slice(0, 80);
		const MIT_LEERZEICHEN =
			'Kultivierung und Versuchsanlage unter Kunstlicht mit Richtwerten für ein geschlossenes System';

		it.each([
			['80 Zeichen ohne Leerzeichen', OHNE_LEERZEICHEN],
			['langer Titel mit Leerzeichen', MIT_LEERZEICHEN]
		])('%s: der ganze Titel steht im title und für Vorlese-Programme im Text', (_, titel) => {
			expect(OHNE_LEERZEICHEN).toHaveLength(80);
			// Unterseite: der Titel ist ein Glied; Hauptseite: der Titel ist das Blatt.
			const unter = kopfleiste(
				'/projects/[slug]/[name]',
				{ slug: 'lang', name: 'teil' },
				{ kopf: { kopf: { title: titel } } }
			);
			const { unmount } = render(Brotkrumen, { kopf: unter, onblatt: () => Promise.resolve() });
			const glied = screen.getByRole('link', { name: titel });
			expect(glied).toHaveAttribute('title', titel);
			expect(document.querySelector('.auslassung .nur-vorlesen')).toHaveTextContent(titel);
			unmount();

			render(Brotkrumen, {
				kopf: kopfleiste('/projects/[slug]', { slug: 'lang' }, { title: titel }),
				onblatt: () => Promise.resolve()
			});
			const blatt = screen.getByRole('button', { name: titel });
			expect(blatt.querySelector('.text')).toHaveTextContent(titel);
			expect(blatt.getAttribute('title')).toContain(titel);
		});

		it('Glieder und Blatt kürzen mit Auslassungspunkten, Symbol und „…“ schrumpfen nie', () => {
			for (const s of ['.mitte a', '.blatt .text']) {
				const r = regel(s);
				expect(r, s).toMatch(/overflow: hidden;/);
				expect(r, s).toMatch(/text-overflow: ellipsis;/);
				expect(r, s).toMatch(/white-space: nowrap;/);
			}
			expect(regel('li')).toMatch(/min-width: 0;/);
			expect(regel('.blatt')).toMatch(/flex: 0 1 auto;/);
			expect(regel('.blatt button')).toMatch(/max-width: 100%;/);
			expect(regel('.wurzel,\n\t.auslassung')).toMatch(/flex: none;/);
		});

		it('Text nur für Vorlese-Programme verbreitert die Seite nie (left: 0)', () => {
			// Der eigentliche Fund bei 360 px: ein 1-px-Kasten ohne positionierten
			// Vorfahren blieb am Zeilenende stehen und zählte für die Seitenbreite.
			const app = readFileSync(join(import.meta.dirname, '../../../app.css'), 'utf8');
			const i = app.indexOf('\n.nur-vorlesen {');
			expect(i).toBeGreaterThan(-1);
			const block = app.slice(i, app.indexOf('}', i));
			expect(block).toMatch(/position: absolute;/);
			expect(block).toMatch(/left: 0;/);
		});
	});
});
