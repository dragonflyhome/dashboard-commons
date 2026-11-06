import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import Verlauf from './Verlauf.svelte';
import type { PcMesswert } from '$vertrag';

function wert(zeit: string, cpu: number | null, mem: number): PcMesswert {
	return { zeit, cpu, mem, platte_belegt: null, platte_gesamt: null };
}

describe('Verlauf', () => {
	it('sagt freundlich, wenn es noch keine Messwerte gibt', () => {
		const { container } = render(Verlauf, { werte: [] });
		expect(
			screen.getByText('Noch keine Messwerte, der erste kommt kurz nach dem Start.')
		).toBeInTheDocument();
		expect(container.querySelector('svg')).toBeNull();
	});
	it('zeigt bei einem einzigen Wert keine Kurve', () => {
		const { container } = render(Verlauf, { werte: [wert('2026-10-25T20:35:00Z', 10, 40)] });
		expect(container.querySelector('svg')).toBeNull();
	});
	it('zeichnet zwei Linien mit Zusammenfassung', () => {
		const { container } = render(Verlauf, {
			werte: [
				wert('2026-10-25T20:25:00Z', 10, 40),
				wert('2026-10-25T20:30:00Z', 20, 50),
				wert('2026-10-25T20:35:00Z', 30, 60)
			]
		});
		const svgs = container.querySelectorAll('svg');
		expect(svgs).toHaveLength(1);
		expect(svgs[0]?.querySelectorAll('path')).toHaveLength(2);
		expect(
			screen.getByRole('img', {
				name: /Prozessor im Mittel 20 Prozent.*Arbeitsspeicher im Mittel 50/
			})
		).toBeInTheDocument();
		expect(svgs[0]?.querySelector('title')?.textContent).toMatch(/PC-Verlauf von/);
	});

	it('Etappe 3a+: unbekannte Prozessorlast ist eine Lücke, keine 0, und zählt nicht ins Mittel', () => {
		const { container } = render(Verlauf, {
			werte: [
				wert('2026-10-25T20:25:00Z', 10, 40),
				wert('2026-10-25T20:30:00Z', null, 50),
				wert('2026-10-25T20:35:00Z', 30, 60)
			]
		});
		const cpu = container.querySelector('path.cpu')?.getAttribute('d') ?? '';
		expect(cpu.match(/M/g)).toHaveLength(2);
		expect(cpu).not.toContain('L');
		expect(container.querySelector('path.mem')?.getAttribute('d')?.match(/L/g)).toHaveLength(2);
		expect(screen.getByRole('img', { name: /Prozessor im Mittel 20 Prozent/ })).toBeInTheDocument();
	});
});
