<script lang="ts">
	/** Kanban aus `daten/kanban.toml`: Spalten mit Karten. */
	import type { Abgeleitet, Kanban } from '$vertrag';
	import KanbanBlock from '#lib/components/blocks/Kanban.svelte';
	import { kanban } from '#lib/daten.ts';
	import Daten from './intern/Daten.svelte';
	import { erledigtSatz, liste, listenPfad, type Quelle } from './abgeleitet.ts';

	interface Props {
		daten: Quelle<Kanban>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
	}
	let { daten, teil, abgeleitet }: Props = $props();
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(k: Kanban, name: string | null, abg: Abgeleitet | null)}
		{@const satz = erledigtSatz(liste(abg, listenPfad('spalte.karte', name)))}
		{#if satz !== null}
			<p class="muted small zusatz">{satz}</p>
		{/if}
		<KanbanBlock columns={kanban(k)} />
	{/snippet}
</Daten>

<style>
	.zusatz {
		margin: 0.4em 0 0;
	}
</style>
