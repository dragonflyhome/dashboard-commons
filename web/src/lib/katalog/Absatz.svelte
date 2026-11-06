<script lang="ts">
	/** Ein Absatz Fließtext, direkt in der Seite geschrieben. */
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}
	let { children }: Props = $props();
</script>

<p class="absatz">{@render children()}</p>

<style>
	.absatz {
		max-width: 72ch;
	}
</style>
