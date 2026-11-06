<script lang="ts">
	/**
	 * Klappfeld (Kopfleiste, Fassung 3, Bits UI Popover): ein Knopf, der ein
	 * kleines Feld mit Inhalt aufklappt, etwa die Liste der Meldungen an der
	 * Glocke. Enter, Leertaste oder Klick öffnen; Escape und ein Klick daneben
	 * schließen, danach steht der Fokus wieder auf dem Knopf. Im Feld geht Tab
	 * durch Links und Knöpfe.
	 *
	 *   <Klappfeld name="Meldungen" titel="Meldungen">
	 *     {#snippet knopf()}🔔{/snippet}
	 *     {#snippet children(schliessen)}<a href="…" onclick={schliessen}>…</a>{/snippet}
	 *   </Klappfeld>
	 *
	 * Ohne Seitensperre (wie das Menü seit dem Hotfix): Die Seite
	 * dahinter bleibt bedienbar und rollt weiter; nichts setzt
	 * `pointer-events` oder `overflow` an `<body>`.
	 */
	import type { Snippet } from 'svelte';
	import { Popover } from 'bits-ui';

	interface Props {
		/** Name des Knopfs für Vorlese-Programme (er zeigt oft nur ein Zeichen). */
		name: string;
		/** Überschrift im Feld. */
		titel: string;
		offen?: boolean;
		/** Was der Knopf zeigt (Zeichen, Zahl). */
		knopf: Snippet;
		/** Inhalt; bekommt `schliessen` (etwa für Links, die die Seite wechseln). */
		children: Snippet<[() => void]>;
		/** Beim Öffnen und Schließen (etwa um Daten nachzuladen). */
		onoffen?: (offen: boolean) => void;
	}
	let { name, titel, offen = $bindable(false), knopf, children, onoffen }: Props = $props();
	const id = $props.id();

	function schliessen(): void {
		offen = false;
	}
</script>

<Popover.Root bind:open={offen} onOpenChange={(o) => onoffen?.(o)}>
	<Popover.Trigger>
		{#snippet child({ props })}
			<button {...props} type="button" class="ausloeser" aria-label={name}>{@render knopf()}</button
			>
		{/snippet}
	</Popover.Trigger>
	<Popover.Portal>
		<Popover.Content sideOffset={6} collisionPadding={8} align="end" preventScroll={false}>
			{#snippet child({ wrapperProps, props, open })}
				{#if open}
					<div {...wrapperProps}>
						<div {...props} class="feld" role="dialog" aria-labelledby="{id}-titel">
							<h2 class="titel" id="{id}-titel">{titel}</h2>
							{@render children(schliessen)}
						</div>
					</div>
				{/if}
			{/snippet}
		</Popover.Content>
	</Popover.Portal>
</Popover.Root>

<style>
	.ausloeser {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 2.75rem;
		min-height: 2.75rem;
		padding: 0 0.25rem;
		border: 0;
		border-radius: 0.625rem;
		background: none;
		color: var(--text);
		font: inherit;
		cursor: pointer;
	}
	.ausloeser:hover,
	.ausloeser[data-state='open'] {
		background: var(--accent-soft);
	}
	.feld {
		z-index: 50;
		width: min(24rem, calc(100vw - 1rem));
		max-height: min(32rem, calc(100vh - 5rem));
		overflow-y: auto;
		padding: 0.75rem 0.875rem;
		border: 1px solid var(--line);
		/* Wie der Dialog des Katalogs: der Eckenradius aus dem Token. */
		border-radius: var(--radius);
		background: var(--card);
		color: var(--text);
		box-shadow: var(--shadow-gross);
		overflow-wrap: anywhere;
	}
	.titel {
		margin: 0 0 0.5rem;
		font-size: var(--text-base);
	}
</style>
