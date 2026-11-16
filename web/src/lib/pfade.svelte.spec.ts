/**
 * Lokale Pfade sind keine Links; angemeldet stehen sie im Klartext da
 * (Schritt E3). Erfundene Namen.
 */
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { SATZ_LOKALER_PFAD, istLokalerPfad } from './pfade.ts';
import LinksListe from './components/projekt/LinksListe.svelte';

describe('istLokalerPfad', () => {
	it('erkennt Laufwerk, file: und Netzwerkpfad', () => {
		for (const p of [
			'C:\\Users\\beispiel\\Downloads\\x.zip',
			'C:/Users/beispiel/Projekte/bericht',
			'd:\\Ablage',
			'file:///C:/Ablage/x.pdf',
			'FILE://rechner/x',
			'\\\\rechner\\freigabe\\x.pdf',
			'  C:\\mit-leerzeichen-vorn'
		]) {
			expect(istLokalerPfad(p), p).toBe(true);
		}
	});
	it('Web-Adressen, mailto und Wege im Dashboard bleiben Links', () => {
		for (const p of [
			'https://example.org/C:/x',
			'http://localhost:8787/',
			'mailto:a@example.org',
			'/projects/x',
			'C:'
		]) {
			expect(istLokalerPfad(p), p).toBe(false);
		}
	});
});

describe('istLokalerPfad unter macOS und Linux', () => {
	it('erkennt Profil- und Systemformen', () => {
		for (const p of [
			'/Users/beispiel/Downloads/x.zip',
			'/home/beispiel/dev',
			'/Volumes/Platte/Ablage',
			'/root',
			'/usr/local/bin/git',
			'/opt/homebrew/bin/node',
			'/private/var/folders/ab/x/T',
			'/Applications',
			'  /Users/beispiel'
		]) {
			expect(istLokalerPfad(p), p).toBe(true);
		}
	});
	it('ein Weg im Dashboard mit / vorn ist nie lokal', () => {
		for (const p of [
			'/projekte/x',
			'/projects/x',
			'/pc',
			'/daten',
			'/anstehend',
			'/',
			'/Userspace/x',
			'/homepage',
			'/users/beispiel',
			'https://example.org/Users/beispiel',
			'ssh://host/Users/beispiel/repo.git'
		]) {
			expect(istLokalerPfad(p), p).toBe(false);
		}
	});
});

describe('LinksListe', () => {
	it('lokaler Pfad: Text mit Hinweis, ohne Link, im Klartext (Schritt E3)', () => {
		const { container } = render(LinksListe, {
			links: [
				{ name: 'Export', url: 'C:\\Users\\beispiel\\Downloads\\export.zip' },
				{ name: 'Ablage', url: 'file:///D:/Ablage' },
				{ name: 'Netz', url: 'https://example.org/seite' }
			]
		});
		const links = [...container.querySelectorAll('a')];
		expect(links.map((a) => a.textContent)).toEqual(['Netz']);
		expect(links[0]?.getAttribute('href')).toBe('https://example.org/seite');
		expect(screen.getByText('Export').closest('a')).toBeNull();
		expect(container.textContent).toContain('C:\\Users\\beispiel\\Downloads\\export.zip');
		expect(container.textContent).not.toContain('%USERPROFILE%');
		expect(screen.getAllByText(SATZ_LOKALER_PFAD)).toHaveLength(2);
	});
});
