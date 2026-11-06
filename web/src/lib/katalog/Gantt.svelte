<script lang="ts">
	/**
	 * Balkenplan aus `daten/gantt.toml` (Daten oder Plan-Monate, mit `start`
	 * umgerechnet). Fehler im Plan erscheinen als Satz statt als Balken.
	 */
	import type { Abgeleitet, Gantt } from '$vertrag';
	import GanttBlock from './zeit/Gantt.svelte';
	// Etappe 3b: Textfassung des Balkenplans gleich darunter.
	import Textfassung from '#lib/components/Textfassung.svelte';
	import { ganttText } from '#lib/textfassung.ts';
	import { gantt } from '#lib/daten.ts';
	import Daten from './intern/Daten.svelte';
	import { erledigtSatz, liste, listenPfad, type Quelle } from './abgeleitet.ts';

	interface Props {
		daten: Quelle<Gantt>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
	}
	let { daten, teil, abgeleitet }: Props = $props();
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(g: Gantt, name: string | null, abg: Abgeleitet | null)}
		{@const plan = gantt(g)}
		{@const satz = erledigtSatz(liste(abg, listenPfad('balken', name)))}
		{#if 'fehler' in plan}
			<p class="block-error">{plan.fehler}</p>
		{:else}
			{#if satz !== null}
				<p class="muted small zusatz">{satz}</p>
			{/if}
			<GanttBlock data={plan} />
			<Textfassung fassung={ganttText(plan)} />
		{/if}
	{/snippet}
</Daten>

<style>
	.zusatz {
		margin: 0.4em 0 0;
	}
</style>
