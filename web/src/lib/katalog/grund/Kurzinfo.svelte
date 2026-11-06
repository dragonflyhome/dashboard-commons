<script lang="ts">
	/**
	 * Kurzinfo (Etappe 3a-3, Bits UI Tooltip): ein kurzer Zusatz, der beim
	 * Zeigen mit der Maus oder beim Fokus mit der Tastatur erscheint; Escape
	 * schließt. Nur für Zusätze: Am Handy gibt es kein Zeigen, darum muss das
	 * Wichtige immer schon im Text stehen. Für längere Hinweise `<Hinweis>`.
	 *
	 *   <Kurzinfo text="Zählt nur Einträge mit Frist" name="Mehr zu Fristen">ⓘ</Kurzinfo>
	 *
	 * `name` ist der Name des Knopfs für Vorlese-Programme (aria-label); er
	 * braucht ihn, sobald der Knopf nur ein Zeichen wie ⓘ zeigt.
	 */
	import type { Snippet } from 'svelte';
	import { Tooltip } from 'bits-ui';

	interface Props {
		text: string;
		/** Name des Knopfs für Vorlese-Programme (Pflicht bei einem Zeichen wie ⓘ). */
		name?: string | undefined;
		/**
		 * Offen von außen setzen: nur für Tests und feste Steuerung. Sonst
		 * öffnet die Blase von selbst beim Zeigen oder Fokus.
		 */
		offen?: boolean;
		/** Was den Zusatz trägt (Wort oder Zeichen); wird ein Knopf. */
		children: Snippet;
	}
	let { text, name, offen = $bindable(false), children }: Props = $props();
	const id = $props.id();
	// Bits UI gibt der Blase keine Rolle; darum hier role="tooltip" und,
	// solange sie offen ist, aria-describedby am Auslöser.
</script>

<Tooltip.Provider delayDuration={300}>
	<Tooltip.Root bind:open={offen}>
		<Tooltip.Trigger>
			{#snippet child({ props })}
				<button
					type="button"
					{...props}
					class="ausloeser"
					aria-label={name}
					aria-describedby={offen ? `${id}-info` : undefined}>{@render children()}</button
				>
			{/snippet}
		</Tooltip.Trigger>
		<Tooltip.Portal>
			<Tooltip.Content sideOffset={6} collisionPadding={8}>
				{#snippet child({ wrapperProps, props, open })}
					{#if open}
						<div {...wrapperProps}>
							<div {...props} class="blase" role="tooltip" id="{id}-info">{text}</div>
						</div>
					{/if}
				{/snippet}
			</Tooltip.Content>
		</Tooltip.Portal>
	</Tooltip.Root>
</Tooltip.Provider>

<style>
	.ausloeser {
		padding: 0 0.2rem;
		border: 0;
		border-bottom: 1px dotted var(--muted);
		background: transparent;
		color: inherit;
		font: inherit;
		cursor: help;
	}
	.blase {
		max-width: min(18rem, calc(100vw - 1rem));
		padding: 0.4rem 0.6rem;
		border-radius: 8px;
		background: var(--text);
		color: var(--card);
		font-size: var(--text-sm);
		box-shadow: var(--shadow-gross);
		z-index: 70;
		overflow-wrap: anywhere;
	}
</style>
