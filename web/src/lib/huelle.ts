/**
 * Die Hülle als Daten, erster Bereich: `kopf.ende`, die rechte Gruppe der
 * Kopfleiste. Das Layout (`routes/+layout.svelte`) zeichnet diese Liste in
 * ihrer Reihenfolge; welches Teil dort steht, steht hier und nicht im Markup.
 *
 * „Ende“ statt „rechts“: Der Name stimmt auch bei Schrift von rechts nach links.
 *
 * Die Reihenfolge ist Absicht. Das Schloss steht am Rand, davor der Live-Punkt:
 * Beide zeigen Zustände, die für die Sicherheit wichtig sind (die Anmeldung, den
 * Kanal). `huelle.svelte.spec.ts` hält Reihenfolge und Rand fest.
 *
 * Ein Teil hat hier nur Kennung und Komponente, und die Komponente bekommt keine
 * Eigenschaften. Weitere Felder kommen erst dazu, wenn Code sie liest.
 */
import type { Component } from 'svelte';
import PcSymbol from '#lib/components/kopf/PcSymbol.svelte';
import Glocke from '#lib/components/kopf/Glocke.svelte';
import LiveBadge from '#lib/components/LiveBadge.svelte';
import Schloss from '#lib/components/Schloss.svelte';

/** Ein Teil der Hülle: eine Kennung und eine Komponente ohne Eigenschaften. */
export interface HuellenTeil {
	readonly id: string;
	readonly teil: Component;
}

/** Die Teile am Ende der Kopfleiste, in der Reihenfolge, in der sie stehen. */
export const KOPF_ENDE = [
	{ id: 'pc', teil: PcSymbol },
	{ id: 'glocke', teil: Glocke },
	{ id: 'live', teil: LiveBadge },
	{ id: 'schloss', teil: Schloss }
] as const satisfies readonly HuellenTeil[];
