import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { PcTick } from '$vertrag';

vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));
vi.mock('$app/state', () => ({ page: { route: { id: '/' } } }));
const { default: PcSymbol } = await import('./PcSymbol.svelte');
const { PcMinute, PLAETZE, pcName } = await import('#lib/pcminute.svelte.ts');

function tick(cpu: number, at: string, belegung: PcTick['cpu_belegung'] = 'normal'): PcTick {
	return {
		at,
		cpu_percent: cpu,
		mem_used: 1,
		mem_total: 2,
		disks: [],
		uptime_secs: 1,
		cpu_belegung: belegung,
		mem_belegung: 'normal'
	};
}

describe('PC-Symbol (Kopfleiste, Fassung 3; die Minilinie hat eigene Specs)', () => {
	it('merkt sich die letzten 12 Werte (60 s), doppelte Zeiten zählen nicht, unbekannt nie', () => {
		const m = new PcMinute();
		for (let i = 0; i < 15; i++) m.aufnehmen(tick(i, `t${String(i)}`));
		expect(m.werte).toHaveLength(PLAETZE);
		expect(m.werte[0]).toBe(3);
		m.aufnehmen(tick(99, 't14'));
		expect(m.werte.at(-1)).toBe(14);
		m.aufnehmen(tick(50, 't15', 'unbekannt'));
		expect(m.werte.at(-1)).toBe(14);
		m.aufnehmen(tick(91.6, 't16', 'kritisch'));
		expect([m.jetzt, m.belegung]).toEqual([92, 'kritisch']);
		m.aufnehmen(null);
		expect(m.werte).toHaveLength(PLAETZE);
	});

	it('Link zu /pc mit Namen „PC: Prozessor N %“, Farbe nach der Stufe vom Server', () => {
		const m = new PcMinute();
		m.aufnehmen(tick(10, 'a'));
		m.aufnehmen(tick(72.4, 'b', 'hoch'));
		render(PcSymbol, { minute: m });
		const link = screen.getByRole('link', { name: 'PC: Prozessor 72 %' });
		expect(link).toHaveAttribute('href', '/pc');
		expect(link).toHaveAttribute('title', 'PC: Prozessor 72 %');
		expect(link).toHaveAttribute('data-belegung', 'hoch');
		const svg = link.querySelector('svg');
		expect(svg).toHaveClass('b-wartet');
		expect(svg).toHaveAttribute('aria-hidden', 'true');
		expect(svg?.querySelector('path.linie')).not.toBeNull();
	});

	it('vor dem ersten Wert: ein Name ohne Zahl, keine Linie', () => {
		render(PcSymbol, { minute: new PcMinute() });
		expect(screen.getByRole('link', { name: pcName(null) })).toBeInTheDocument();
		expect(document.querySelector('path.linie')).toBeNull();
	});
});
