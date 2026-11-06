<script lang="ts">
	/**
	 * Reiter: `titel` nennt die Reiter, `children` bekommt die Stelle des
	 * gewählten Reiters und zeigt den passenden Inhalt.
	 *
	 *   <Reiter titel={['Plan', 'Stand']}>
	 *     {#snippet children(gewaehlt)}
	 *       {#if gewaehlt === 0}<Gantt daten={plan} />{:else}<Fortschritt daten={stand} />{/if}
	 *     {/snippet}
	 *   </Reiter>
	 *
	 * Seit Etappe 3a-3 auf Bits UI Tabs: Pfeil links und rechts wechseln den
	 * Reiter (im Kreis), Pos1 und Ende springen zum ersten und letzten; Rollen
	 * tablist, tab und tabpanel wie bisher. Die Schnittstelle blieb gleich;
	 * das Einblenden entfiel (ruhig, keine Bewegung außer beim Laden).
	 * Nur der gewählte Inhalt steht im Dokument; darum zeigt aria-controls
	 * nur beim gewählten Reiter auf ihn (sonst zeigte es ins Leere).
	 */
	import type { Snippet } from 'svelte';
	import { Tabs } from 'bits-ui';

	interface Props {
		titel: readonly string[];
		children: Snippet<[number]>;
	}
	let { titel, children }: Props = $props();

	let wert = $state('0');
	// Wird die Liste kürzer als der gewählte Reiter, gilt wieder der erste;
	// sonst stünde kein Inhalt da.
	const aktiv = $derived(Number(wert) < titel.length ? Number(wert) : 0);
	$effect(() => {
		if (String(aktiv) !== wert) wert = String(aktiv);
	});
</script>

<Tabs.Root bind:value={wert}>
	{#snippet child({ props })}
		<div {...props} class="reiter">
			<Tabs.List>
				{#snippet child({ props: liste })}
					<div {...liste} class="liste">
						{#each titel as t, i (i)}
							<Tabs.Trigger value={String(i)}>
								{#snippet child({ props: reiter })}
									<button
										{...reiter}
										class="tab"
										class:aktiv={aktiv === i}
										aria-controls={aktiv === i
											? (reiter['aria-controls'] as string | undefined)
											: undefined}>{t}</button
									>
								{/snippet}
							</Tabs.Trigger>
						{/each}
					</div>
				{/snippet}
			</Tabs.List>
			{#each titel as t, i (`${String(i)}-${t}`)}
				<Tabs.Content value={String(i)}>
					{#snippet child({ props: inhalt })}
						{#if aktiv === i}
							<div {...inhalt} class="inhalt">
								{@render children(i)}
							</div>
						{/if}
					{/snippet}
				</Tabs.Content>
			{/each}
		</div>
	{/snippet}
</Tabs.Root>

<style>
	.reiter {
		margin: 0.6em 0 1em;
		border: 1px solid var(--line);
		border-radius: 12px;
		overflow: hidden;
	}
	.liste {
		display: flex;
		flex-wrap: wrap;
		background: color-mix(in srgb, var(--line) 35%, var(--card));
		border-bottom: 1px solid var(--line);
	}
	.tab {
		border: 0;
		background: transparent;
		color: var(--muted);
		font: inherit;
		font-weight: 600;
		padding: 9px 16px;
		cursor: pointer;
		border-bottom: 3px solid transparent;
		margin-bottom: -1px;
		transition:
			color 120ms ease,
			border-color 120ms ease;
	}
	.tab:hover {
		color: var(--text);
	}
	.tab.aktiv {
		color: var(--accent);
		border-bottom-color: var(--accent);
		background: var(--card);
	}
	.inhalt {
		padding: 10px 14px;
		min-width: 0;
	}
</style>
