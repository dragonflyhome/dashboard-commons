<script lang="ts">
	/**
	 * Die Zeilen einer Fassung aus dem Changelog (Seite /fassungen und „Neu
	 * in …“): je Unterabschnitt eine Überschrift und eine Aufzählung; eine
	 * Zeile „Bruch: …“ trägt das Wort hervorgehoben.
	 */
	import type { FassungsZeilen } from '$vertrag';
	import { abschnitteMitZeilen, bruchTeilen } from '#lib/fassungen.ts';

	interface Props {
		zeilen: FassungsZeilen;
		/** Überschriftenebene der Unterabschnitte (unter einem h2 also 3). */
		ebene?: 3 | 4;
	}
	let { zeilen, ebene = 3 }: Props = $props();
	const abschnitte = $derived(abschnitteMitZeilen(zeilen));
</script>

{#each abschnitte as a (a.titel)}
	<svelte:element this={`h${ebene}`} class="unter">{a.titel}</svelte:element>
	<ul class="zeilen">
		{#each a.zeilen as z, i (i)}
			{@const t = bruchTeilen(z)}
			<li>
				{#if t.bruch}<strong class="bruch">Bruch:</strong> {t.rest}{:else}{t.rest}{/if}
			</li>
		{/each}
	</ul>
{:else}
	<p class="muted">Keine Einträge.</p>
{/each}

<style>
	.unter {
		margin: 0.8em 0 0.2em;
		font-size: var(--text-base);
	}
	.zeilen {
		margin: 0 0 0.6em;
		padding-left: 1.4em;
		display: grid;
		gap: 4px;
	}
	.bruch {
		color: var(--bad-text);
	}
</style>
