import { describe, expect, it } from 'vitest';
import { KOPF_ENDE } from './huelle.ts';
import PcSymbol from '#lib/components/kopf/PcSymbol.svelte';
import Glocke from '#lib/components/kopf/Glocke.svelte';
import LiveBadge from '#lib/components/LiveBadge.svelte';
import Schloss from '#lib/components/Schloss.svelte';

describe('KOPF_ENDE, die Teile am Ende der Kopfleiste', () => {
	it('die Kennungen stehen in dieser Reihenfolge: pc, glocke, live, schloss', () => {
		expect(KOPF_ENDE.map((t) => t.id)).toEqual(['pc', 'glocke', 'live', 'schloss']);
	});

	it('jede Kennung trägt ihre Komponente: PcSymbol, Glocke, LiveBadge, Schloss', () => {
		expect(KOPF_ENDE.map((t) => t.teil)).toEqual([PcSymbol, Glocke, LiveBadge, Schloss]);
	});

	it('keine Kennung kommt zweimal vor', () => {
		const kennungen = KOPF_ENDE.map((t) => t.id);
		expect(new Set(kennungen).size).toBe(kennungen.length);
	});

	it('das Schloss ist der letzte Eintrag, der Live-Punkt steht direkt davor', () => {
		expect(KOPF_ENDE.at(-1)?.teil).toBe(Schloss);
		expect(KOPF_ENDE.at(-2)?.teil).toBe(LiveBadge);
	});
});
