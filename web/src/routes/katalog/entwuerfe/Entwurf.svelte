<script lang="ts">
	/** Ein Entwurf eines Widgets im Katalog: Name, ob gewählt, ein Satz dazu. */
	import type { Snippet } from 'svelte';

	interface Props {
		name: string;
		gewaehlt?: boolean;
		satz: string;
		children: Snippet;
	}
	let { name, gewaehlt = false, satz, children }: Props = $props();
</script>

<figure class="entwurf" class:gewaehlt>
	<figcaption>
		<b>„{name}“</b>
		{#if gewaehlt}<span class="pill gewaehlt-schild">gewählt</span>{/if}
		<span class="muted small satz">{satz}</span>
	</figcaption>
	<div class="buehne">{@render children()}</div>
</figure>

<style>
	.entwurf {
		margin: 0;
		min-width: 0;
		display: grid;
		gap: 0.4rem;
		align-content: start;
	}
	figcaption {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.2rem 0.5rem;
	}
	.satz {
		flex-basis: 100%;
	}
	.gewaehlt-schild {
		background: var(--accent-soft);
		color: var(--accent-text);
		font-weight: 600;
	}
	.buehne {
		display: flex;
		min-width: 0;
	}
	.gewaehlt .buehne {
		outline: 2px solid var(--accent);
		outline-offset: 4px;
		border-radius: var(--radius);
	}
</style>
