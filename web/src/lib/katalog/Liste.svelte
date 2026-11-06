<script lang="ts">
	/** Eine Aufzählung; `nummeriert` für eine Reihenfolge. */
	interface Props {
		punkte: readonly string[];
		nummeriert?: boolean | undefined;
	}
	let { punkte, nummeriert = false }: Props = $props();
</script>

{#if nummeriert}
	<ol class="liste">
		{#each punkte as p, i (i)}
			<li>{p}</li>
		{/each}
	</ol>
{:else}
	<ul class="liste">
		{#each punkte as p, i (i)}
			<li>{p}</li>
		{/each}
	</ul>
{/if}

<style>
	.liste {
		margin: 0.4em 0 1em;
		padding-left: 1.4em;
		display: grid;
		gap: 4px;
	}
</style>
