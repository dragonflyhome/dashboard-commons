/**
 * Lokale Pfade sind keine Links, und der Kontoname steht
 * nirgends im Bild (Links-Kästen, /pc). Erfundene Namen.
 */
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { PROFIL_PLATZHALTER, SATZ_LOKALER_PFAD, istLokalerPfad, profilVerbergen } from './pfade.ts';
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

describe('profilVerbergen', () => {
	it('ersetzt das Profil, behält den Rest', () => {
		expect(profilVerbergen('C:\\Users\\beispiel\\Downloads\\x.zip')).toBe(
			`${PROFIL_PLATZHALTER}\\Downloads\\x.zip`
		);
		expect(profilVerbergen('c:/users/Beispiel Konto/Projekte')).toBe(
			`${PROFIL_PLATZHALTER}/Projekte`
		);
		expect(profilVerbergen('D:\\Benutzer\\beispiel')).toBe(PROFIL_PLATZHALTER);
		expect(profilVerbergen('file:///C:/Users/beispiel/x.pdf')).toBe(`${PROFIL_PLATZHALTER}/x.pdf`);
	});
	it('mehrere Stellen in einem Befehl, Anführungszeichen bleiben', () => {
		expect(
			profilVerbergen('"C:\\Users\\beispiel\\AppData\\a.exe" --ort C:\\Users\\beispiel\\b')
		).toBe(`"${PROFIL_PLATZHALTER}\\AppData\\a.exe" --ort ${PROFIL_PLATZHALTER}\\b`);
	});
	it('andere Pfade bleiben, wie sie sind', () => {
		for (const p of [
			'C:\\ProgramData\\Dashboard\\dienst.toml',
			'C:\\Users',
			'D:/Ablage/Projekte',
			'kein Pfad'
		]) {
			expect(profilVerbergen(p)).toBe(p);
		}
	});
});

describe('LinksListe', () => {
	it('lokaler Pfad: Text mit Hinweis, ohne Link und ohne Kontonamen', () => {
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
		expect(container.textContent).not.toContain('beispiel');
		expect(container.textContent).toContain(`${PROFIL_PLATZHALTER}\\Downloads\\export.zip`);
		expect(screen.getAllByText(SATZ_LOKALER_PFAD)).toHaveLength(2);
	});
});
