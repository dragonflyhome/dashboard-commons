<script lang="ts">
	/**
	 * Ein Anzeige-Baustein (`Block` aus dem Vertrag), wie `NeueForm` ihn aus
	 * `daten/*.toml` macht (`#lib/daten.ts`).
	 */
	import type { Block } from '$vertrag';
	import Progress from './blocks/Progress.svelte';
	import Ampel from './blocks/Ampel.svelte';
	import Kpi from './blocks/Kpi.svelte';
	import Kanban from './blocks/Kanban.svelte';
	import Flow from './blocks/Flow.svelte';
	// Etappe 3a-1: Gantt auf der gemeinsamen Zeitachse.
	import Gantt from '#lib/katalog/zeit/Gantt.svelte';
	// Etappe 3b: Textfassung des Balkenplans gleich darunter.
	import Textfassung from './Textfassung.svelte';
	import { ganttText } from '#lib/textfassung.ts';

	interface Props {
		block: Block;
	}
	let { block: b }: Props = $props();
</script>

{#if b.kind === 'progress'}
	<Progress rows={b.rows} />
{:else if b.kind === 'ampel'}
	<Ampel rows={b.rows} />
{:else if b.kind === 'kpi'}
	<Kpi items={b.items} />
{:else if b.kind === 'kanban'}
	<Kanban columns={b.columns} />
{:else if b.kind === 'gantt'}
	<Gantt data={b} />
	<Textfassung fassung={ganttText(b)} />
{:else if b.kind === 'flow'}
	<Flow data={b} />
{:else}
	<p class="block-error">⚠ {b.message}</p>
{/if}
