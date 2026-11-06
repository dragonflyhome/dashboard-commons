<script lang="ts">
	/**
	 * Aufklapper (Etappe 3a-3, Bits UI Collapsible): ein Abschnitt, der auf-
	 * und zuklappt. Der Kopf ist ein Knopf (Enter, Leertaste) mit
	 * aria-expanded; zu ist der Inhalt nicht im Dokument, darum zeigt
	 * aria-controls erst offen auf ihn. Für Einträge mit
	 * Anker (`#frage-…`) bleibt `<details>` richtig, weil ansteuern() es
	 * aufklappt.
	 *
	 *   <Aufklapper titel="Ältere Einträge">…</Aufklapper>
	 */
	import type { Snippet } from 'svelte';
	import { Collapsible } from 'bits-ui';

	interface Props {
		titel: string;
		/** Offen; von außen etwa für einen Abschnitt, der zu Beginn offen steht. */
		offen?: boolean;
		children: Snippet;
	}
	let { titel, offen = $bindable(false), children }: Props = $props();
</script>

<Collapsible.Root bind:open={offen}>
	{#snippet child({ props })}
		<div {...props} class="aufklapper">
			<Collapsible.Trigger>
				{#snippet child({ props: kopf })}
					<button
						{...kopf}
						class="kopf"
						aria-controls={offen ? (kopf['aria-controls'] as string | undefined) : undefined}
					>
						<span class="pfeil" class:offen aria-hidden="true">▸</span>
						{titel}
					</button>
				{/snippet}
			</Collapsible.Trigger>
			<Collapsible.Content>
				{#snippet child({ props: inhalt, open })}
					{#if open}
						<div {...inhalt} class="inhalt">{@render children()}</div>
					{/if}
				{/snippet}
			</Collapsible.Content>
		</div>
	{/snippet}
</Collapsible.Root>

<style>
	.aufklapper {
		margin: 0.4em 0;
		border: 1px solid var(--line);
		border-radius: 10px;
	}
	.kopf {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		width: 100%;
		min-height: 2.5rem;
		padding: 0.4rem 0.75rem;
		border: 0;
		border-radius: 10px;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-weight: 600;
		text-align: left;
		cursor: pointer;
	}
	.kopf:hover {
		background: var(--accent-soft);
	}
	.pfeil {
		color: var(--muted);
		transition: transform 120ms ease;
	}
	.pfeil.offen {
		transform: rotate(90deg);
	}
	.inhalt {
		padding: 0 0.75rem 0.6rem;
		min-width: 0;
	}
</style>
