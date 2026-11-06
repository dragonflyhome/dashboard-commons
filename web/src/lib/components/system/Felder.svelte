<script lang="ts">
	/**
	 * Die übrigen Felder einer Datei aus docs/ (Etappe 4d) als einfacher Text:
	 * je Feld der Name als kleine Überschrift, darunter Text (Absätze) oder
	 * eine Aufzählung. Nur Bausteine aus dem Katalog, kein Markdown.
	 */
	import { Liste, Text } from '$katalog';
	import type { Feld } from '$vertrag';
	import { zeilen } from '#lib/entscheidungen.ts';

	interface Props {
		felder: readonly Feld[];
	}
	let { felder }: Props = $props();
	const liste = $derived(zeilen(felder));
</script>

{#each liste as z, i (i)}
	<h3 class="feld">{z.name}</h3>
	{#if z.punkte !== undefined}
		{#if z.punkte.length === 0}
			<p class="muted small">leer</p>
		{:else}
			<Liste punkte={z.punkte} />
		{/if}
	{:else}
		<Text text={z.text ?? ''} />
	{/if}
{/each}

<style>
	.feld {
		margin: 0.9em 0 0.2em;
		font-size: var(--text-sm);
	}
</style>
