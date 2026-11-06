<script lang="ts">
	import { slide } from 'svelte/transition';
	import type { KanbanColumn } from '$vertrag';
	import StatusDot from '../StatusDot.svelte';

	interface Props {
		columns: KanbanColumn[];
	}
	let { columns }: Props = $props();

	let collapsed = $state<Record<string, boolean>>({});
	function toggle(title: string): void {
		collapsed[title] = !(collapsed[title] ?? false);
	}
</script>

<div class="kanban">
	{#each columns as c (c.title)}
		{@const closed = collapsed[c.title] ?? false}
		<section class="col">
			<button
				type="button"
				class="head"
				aria-expanded={!closed}
				onclick={() => {
					toggle(c.title);
				}}
			>
				<StatusDot bedeutung={c.bedeutung} />
				<span class="title">{c.title}</span>
				<span class="count">{c.items.length}</span>
				<span class="chev" aria-hidden="true">{closed ? '▸' : '▾'}</span>
			</button>
			{#if !closed}
				<div class="items" transition:slide={{ duration: 180 }}>
					{#each c.items as it, i (i)}
						<div class="kcard">{it}</div>
					{/each}
				</div>
			{/if}
		</section>
	{/each}
</div>

<style>
	.kanban {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
		gap: 10px;
		margin: 0.6em 0 1em;
		align-items: start;
	}
	.col {
		border: 1px solid var(--line);
		border-radius: 12px;
		background: color-mix(in srgb, var(--line) 25%, var(--card));
	}
	.head {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		border: 0;
		background: transparent;
		color: inherit;
		font: inherit;
		font-weight: 600;
		padding: 8px 10px;
		cursor: pointer;
		text-align: left;
	}
	.title {
		flex: 1;
	}
	.count {
		font-size: var(--text-xs);
		color: var(--muted);
	}
	.items {
		display: grid;
		gap: 6px;
		padding: 0 8px 8px;
	}
	.kcard {
		background: var(--card);
		border: 1px solid var(--line);
		border-radius: 8px;
		padding: 6px 9px;
		font-size: var(--text-md);
	}
</style>
