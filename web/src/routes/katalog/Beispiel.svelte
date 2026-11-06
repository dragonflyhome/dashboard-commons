<script lang="ts">
	/**
	 * Rahmen für einen Baustein im Katalog: Name, kurzer Satz, das lebende
	 * Beispiel und darunter zum Aufklappen, wie man ihn benutzt.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		/** Anker im Katalog, etwa `knopf`. */
		id: string;
		name: string;
		beschreibung: string;
		/** Woher das Verhalten kommt, etwa „Bits UI Switch“ oder „eigen“. */
		herkunft?: string | undefined;
		/** So benutzt man ihn (Svelte, als Text gezeigt). */
		code: string;
		children: Snippet;
	}
	let { id, name, beschreibung, herkunft, code, children }: Props = $props();
</script>

<article class="card beispiel" {id} aria-labelledby="{id}-name">
	<header>
		<h3 id="{id}-name">{name}</h3>
		{#if herkunft}<span class="pill">{herkunft}</span>{/if}
	</header>
	<p class="beschreibung">{beschreibung}</p>
	<div class="lebend">{@render children()}</div>
	<details class="code">
		<summary>So benutzt</summary>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex (der Code rollt seitlich im eigenen Rahmen und ist darum per Tab erreichbar) -->
		<pre tabindex="0" aria-label="Beispielcode {name}"><code>{code}</code></pre>
	</details>
</article>

<style>
	.beispiel {
		min-width: 0;
		scroll-margin-top: 5rem;
	}
	header {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.3rem 0.6rem;
	}
	h3 {
		margin: 0;
	}
	.beschreibung {
		margin: 0.3rem 0 0.75rem;
		color: var(--muted);
	}
	.lebend {
		min-width: 0;
		padding: 0.75rem;
		border: 1px dashed var(--line);
		border-radius: 10px;
	}
	.code {
		margin-top: 0.6rem;
	}
	.code summary {
		cursor: pointer;
		color: var(--accent);
	}
	pre {
		margin: 0.4rem 0 0;
		font-size: var(--text-sm);
	}
	code {
		background: transparent;
		padding: 0;
		white-space: pre;
	}
</style>
