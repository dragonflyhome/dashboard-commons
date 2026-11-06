<script lang="ts">
	/**
	 * Langer Text aus TOML (`text = """…"""`) als Absätze: eine Leerzeile
	 * beginnt einen neuen Absatz. Keine Auszeichnung, kein HTML.
	 */
	interface Props {
		text: string;
	}
	let { text }: Props = $props();

	const absaetze = $derived(
		text
			.replaceAll('\r\n', '\n')
			.split(/\n\s*\n/)
			.map((a) => a.trim())
			.filter((a) => a !== '')
	);
</script>

{#each absaetze as a, i (i)}
	<p class="absatz">{a}</p>
{/each}

<style>
	.absatz {
		max-width: 72ch;
		white-space: pre-line;
	}
</style>
