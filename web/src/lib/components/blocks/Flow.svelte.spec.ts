import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import Flow from './Flow.svelte';
import type { FlowData } from '$vertrag';

const data: FlowData = {
	nodes: [
		{ id: 'Start', label: 'Start', layer: 0, slot: 0 },
		{ id: 'Prüfen', label: 'Prüfen', layer: 1, slot: 0 },
		{ id: 'Ende', label: 'Entscheidung über die Beete im Herbst', layer: 2, slot: 0 }
	],
	edges: [
		[0, 1],
		[1, 2]
	]
};

describe('Flow', () => {
	it('zeichnet Kästen, bricht lange Texte um und hält Auswahl per Klick fest', async () => {
		render(Flow, { data });
		const start = screen.getByRole('button', { name: 'Start' });
		expect(start).toHaveAttribute('aria-pressed', 'false');
		await fireEvent.click(start);
		expect(start).toHaveAttribute('aria-pressed', 'true');
		expect(document.querySelectorAll('tspan').length).toBe(2);
		expect(document.querySelectorAll('.edge.active').length).toBe(1);
	});
});
