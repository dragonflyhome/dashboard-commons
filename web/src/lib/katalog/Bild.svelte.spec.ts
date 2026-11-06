import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { Bild } from './index.ts';
import { bildAdresse, bildFehler } from './bild.ts';
import BildProbe from './intern/BildProbe.svelte';

describe('Bild', () => {
	it('baut die Adresse aus dem Projekt-Kontext und setzt alt', () => {
		render(BildProbe, {
			slug: 'beispiel',
			datei: 'foto_2025-06.jpg',
			alt: 'Porträt im Freien'
		});
		const img = screen.getByRole('img', { name: 'Porträt im Freien' });
		expect(img).toHaveAttribute('src', '/projects/beispiel/assets/foto_2025-06.jpg');
		expect(img).toHaveAttribute('alt', 'Porträt im Freien');
		expect(img).toHaveAttribute('loading', 'lazy');
		expect(img).toHaveAttribute('decoding', 'async');
	});

	it('zeigt Titel und Beschriftung unter dem Bild, Breite als Klasse', () => {
		const { container } = render(BildProbe, {
			slug: 'beispiel',
			datei: 'foto.webp',
			alt: 'Foto',
			titel: 'Im Labor',
			beschriftung: 'Praktikum',
			breite: 'klein'
		});
		expect(screen.getByText('Im Labor')).toBeInTheDocument();
		expect(screen.getByText('Praktikum')).toBeInTheDocument();
		expect(container.querySelector('figure')).toHaveClass('breite-klein');
	});

	it.each([
		'https://example.org/foto.jpg',
		'../anderes/assets/foto.jpg',
		'unter/foto.jpg',
		'.versteckt.jpg',
		'mit leer.jpg',
		'Übersicht.png',
		'con.png'
	])('ungültiger Name „%s“ zeigt einen Satz statt des Bildes', (datei) => {
		render(BildProbe, { slug: 'beispiel', datei, alt: 'Foto' });
		expect(screen.queryByRole('img')).toBeNull();
		expect(screen.getByRole('alert')).toHaveTextContent('ist kein erlaubter Dateiname');
	});

	it('keine Bild-Endung und leeres alt ergeben je einen Satz', () => {
		const a = render(BildProbe, { slug: 'p', datei: 'bericht.pdf', alt: 'Bericht' });
		expect(screen.getByRole('alert')).toHaveTextContent('„bericht.pdf“ ist kein Bild.');
		a.unmount();
		render(BildProbe, { slug: 'p', datei: 'foto.jpg', alt: '  ' });
		expect(screen.getByRole('alert')).toHaveTextContent('Bild braucht `alt`');
	});

	it('Datei kommt nicht: Satz statt kaputtem Bild', async () => {
		render(BildProbe, { slug: 'p', datei: 'fehlt.png', alt: 'Fehlt' });
		await fireEvent.error(screen.getByRole('img'));
		expect(screen.getByRole('alert')).toHaveTextContent('„fehlt.png“ lässt sich nicht laden');
	});

	it('außerhalb einer Projektseite ein verständlicher Fehler', () => {
		expect(() => render(Bild, { datei: 'foto.jpg', alt: 'Foto' })).toThrow(
			/Bild geht nur in einer Projektseite/
		);
	});

	it('Regeln wie der Server', () => {
		expect(bildFehler('Foto-1_a.JPG')).toBeNull();
		expect(bildFehler('')).toMatch(/braucht `datei`/);
		expect(bildFehler(`${'a'.repeat(125)}.png`)).toMatch(/kein erlaubter Dateiname/);
		expect(bildAdresse('mein-projekt', 'a.png')).toBe('/projects/mein-projekt/assets/a.png');
	});
});
