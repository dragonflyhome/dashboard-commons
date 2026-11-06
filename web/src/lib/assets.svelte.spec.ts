import { render, screen } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AssetStand, assetStand, mitVersion } from './assets.svelte.ts';
import { live, type InhaltAenderung } from './live.svelte.ts';
import { freierKanal } from './freikanal.svelte.ts';
import BildProbe from './katalog/intern/BildProbe.svelte';

// Live-Lücken, Punkt 5: ein geändertes Bild in assets/ zeigt sich ohne
// Neuladen, weil seine Adresse eine neue Version bekommt.

const aenderung = (slug: string, version: number, assets: boolean): InhaltAenderung => ({
	projekte: [slug],
	daten: [],
	version,
	sammel: false,
	...(assets ? { assets: [slug] } : {})
});

afterEach(() => {
	assetStand.vergessen();
	vi.restoreAllMocks();
});

describe('mitVersion', () => {
	it('hängt v an, ohne Version bleibt die Adresse', () => {
		expect(mitVersion('/projects/a/assets/b.png', null)).toBe('/projects/a/assets/b.png');
		expect(mitVersion('/projects/a/assets/b.png', 7)).toBe('/projects/a/assets/b.png?v=7');
		expect(mitVersion('/x.html?y=1', 8)).toBe('/x.html?y=1&v=8');
	});
});

describe('AssetStand', () => {
	it('merkt sich nur Änderungen mit assets, je Projekt', () => {
		const s = new AssetStand();
		s.aufnehmen(aenderung('a', 3, false));
		expect(s.version('a')).toBeNull();
		s.aufnehmen(aenderung('a', 4, true));
		expect(s.version('a')).toBe(4);
		expect(s.version('b')).toBeNull();
	});

	it('je Datei: nur die genannten Dateien bekommen die neue Version', () => {
		const s = new AssetStand();
		s.aufnehmen({ ...aenderung('a', 4, true), assetDateien: ['foto.jpg'] });
		expect(s.version('a', 'foto.jpg')).toBe(4);
		expect(s.version('a', 'anderes.png')).toBeNull();
		// Ohne Dateinamen (Vorschau): die neueste Version des Projekts.
		expect(s.version('a')).toBe(4);
		// Eine Änderung ohne Namen gilt für alle Dateien, die neuere Version zählt.
		s.aufnehmen(aenderung('a', 6, true));
		expect(s.version('a', 'anderes.png')).toBe(6);
		expect(s.version('a', 'foto.jpg')).toBe(6);
		s.aufnehmen({ ...aenderung('a', 9, true), assetDateien: ['foto.jpg'] });
		expect(s.version('a', 'foto.jpg')).toBe(9);
		expect(s.version('a', 'anderes.png')).toBe(6);
		expect(s.version('a')).toBe(9);
	});

	it('hört auf beide Kanäle', () => {
		const hoerer: ((a: InhaltAenderung) => void)[] = [];
		vi.spyOn(live, 'onInhalt').mockImplementation((l) => {
			hoerer.push(l);
			return () => undefined;
		});
		vi.spyOn(freierKanal, 'onInhalt').mockImplementation((l) => {
			hoerer.push(l);
			return () => undefined;
		});
		const s = new AssetStand();
		const ab = s.start();
		expect(hoerer).toHaveLength(2);
		hoerer[0]?.(aenderung('a', 5, true));
		hoerer[1]?.(aenderung('b', 6, true));
		expect([s.version('a'), s.version('b')]).toEqual([5, 6]);
		ab();
	});

	it('Bild bekommt nach einer Änderung in assets/ eine neue Adresse', () => {
		render(BildProbe, { slug: 'beispiel', datei: 'foto.jpg', alt: 'Foto' });
		const img = screen.getByRole('img', { name: 'Foto' });
		expect(img).toHaveAttribute('src', '/projects/beispiel/assets/foto.jpg');
		// Ein anderes Projekt ändert nichts.
		assetStand.aufnehmen(aenderung('anderes', 11, true));
		flushSync();
		expect(img).toHaveAttribute('src', '/projects/beispiel/assets/foto.jpg');
		assetStand.aufnehmen(aenderung('beispiel', 12, true));
		flushSync();
		expect(screen.getByRole('img', { name: 'Foto' })).toHaveAttribute(
			'src',
			'/projects/beispiel/assets/foto.jpg?v=12'
		);
		// Eine andere Datei desselben Projekts lässt das Bild in Ruhe.
		assetStand.aufnehmen({ ...aenderung('beispiel', 13, true), assetDateien: ['plan.png'] });
		flushSync();
		expect(screen.getByRole('img', { name: 'Foto' })).toHaveAttribute(
			'src',
			'/projects/beispiel/assets/foto.jpg?v=12'
		);
		assetStand.aufnehmen({ ...aenderung('beispiel', 14, true), assetDateien: ['foto.jpg'] });
		flushSync();
		expect(screen.getByRole('img', { name: 'Foto' })).toHaveAttribute(
			'src',
			'/projects/beispiel/assets/foto.jpg?v=14'
		);
	});
});
