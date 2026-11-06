import { render, screen } from '@testing-library/svelte';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import Minilinie from './Minilinie.svelte';

// Kopfleiste, Fassung 3: die Minilinie im PC-Symbol (eigene Specs, Auflage aus der Abnahme).
const quelle = readFileSync(join(import.meta.dirname, 'Minilinie.svelte'), 'utf8');

/** Die Punkte des Pfads als [x, y]. */
function punkte(): [number, number][] {
	const d = document.querySelector('path.linie')?.getAttribute('d') ?? '';
	return [...d.matchAll(/[ML] ([\d.]+) ([\d.]+)/g)].map((m) => [Number(m[1]), Number(m[2])]);
}

/** y der Grundlinie. */
function grund(): number {
	return Number(document.querySelector('line.grund')?.getAttribute('y1'));
}

describe('Minilinie (Katalog, Kopfleiste Fassung 3)', () => {
	it('Punkte rechtsbündig; mit Beschriftung ein Bild mit Namen', () => {
		render(Minilinie, { werte: [0, 100], plaetze: 12, beschriftung: 'Prozessor zuletzt 100 %' });
		const bild = screen.getByRole('img', { name: 'Prozessor zuletzt 100 %' });
		expect(bild).not.toHaveAttribute('aria-hidden');
		expect(bild).toHaveAttribute('data-werte', '2');
		// Zwei Werte auf 12 Plätzen: der erste steht auf Platz 11 von 12 (x ≈ 29,1), der letzte rechts.
		expect(bild.querySelector('path.linie')?.getAttribute('d')).toBe('M 29.1 13.0 L 32.0 1.0');
		expect(bild.querySelector('circle.jetzt')).toHaveAttribute('cx', '32');
	});

	it('ohne Beschriftung Schmuck (aria-hidden), die Farbe aus der Bedeutung', () => {
		render(Minilinie, { werte: [10, 20, 30], bedeutung: 'wartet' });
		const svg = document.querySelector('svg.minilinie');
		expect(svg).toHaveAttribute('aria-hidden', 'true');
		expect(svg).not.toHaveAttribute('role');
		expect(svg).toHaveClass('b-wartet');
		expect(screen.queryByRole('img')).toBeNull();
	});

	it('ein Wert: nur der Punkt, keine Linie; keine Werte: nur die Grundlinie', () => {
		const r = render(Minilinie, { werte: [40] });
		expect(document.querySelector('path.linie')).toBeNull();
		expect(document.querySelector('circle.jetzt')).not.toBeNull();
		r.unmount();
		render(Minilinie, { werte: [] });
		expect(document.querySelector('circle.jetzt')).toBeNull();
		expect(document.querySelector('line.grund')).not.toBeNull();
	});

	it('wenig Last lesbar: 0 % liegt sichtbar über der Grundlinie, der Strich berührt sie nicht', () => {
		// Live-Probe: bei 13 % lag die Linie fast unsichtbar am unteren Rand.
		render(Minilinie, { werte: [0, 13], plaetze: 2 });
		const [[, null_], [, dreizehn]] = punkte() as [[number, number], [number, number]];
		const unten = grund();
		expect(unten).toBe(15.5);
		// Mindestens 2 px Luft zwischen 0 % und der Grundlinie.
		expect(unten - null_).toBeGreaterThanOrEqual(2);
		// Halber Strich der Linie (1,75) und halbe Grundlinie (1) überlappen nicht.
		expect(null_ + 1.75 / 2).toBeLessThan(unten - 0.5);
		// 13 % ist sichtbar über 0 %.
		expect(null_ - dreizehn).toBeGreaterThan(1);
	});

	it('ehrlich: eine lineare Skala von 0 bis 100 %, wenig Last wird nicht gestreckt', () => {
		render(Minilinie, { werte: [0, 13, 50, 100], plaetze: 4 });
		const y = punkte().map(([, v]) => v);
		const spanne = (y[0] ?? 0) - (y[3] ?? 0);
		expect(y[3]).toBe(1);
		// Gleiche Prozente, gleiche Höhe (auf eine Nachkommastelle gerundet im Pfad).
		expect(((y[0] ?? 0) - (y[1] ?? 0)) / spanne).toBeCloseTo(0.13, 1);
		expect(((y[0] ?? 0) - (y[2] ?? 0)) / spanne).toBeCloseTo(0.5, 2);
	});

	it('Werte außerhalb von 0 bis 100 werden begrenzt', () => {
		render(Minilinie, { werte: [-20, 180], plaetze: 2 });
		expect(punkte()).toEqual([
			[0, 13],
			[32, 1]
		]);
	});

	it('nur Token: Grundlinie in --muted, Linie und Punkt im Ton, ohne Farbwerte; ruhig bei reduzierter Bewegung', () => {
		expect(quelle).toMatch(/\.grund\s*\{[^}]*stroke: var\(--muted\)/);
		expect(quelle).toMatch(/\.linie\s*\{[^}]*stroke: var\(--ton, var\(--muted\)\)/);
		expect(quelle).toMatch(/\.jetzt\s*\{[^}]*fill: var\(--ton, var\(--muted\)\)/);
		expect(quelle).not.toMatch(/#[0-9a-f]{3,6}\b|rgb\(|hsl\(/i);
		expect(quelle).toMatch(
			/prefers-reduced-motion: reduce\)\s*\{\s*\.jetzt\s*\{\s*animation: none/
		);
	});
});
