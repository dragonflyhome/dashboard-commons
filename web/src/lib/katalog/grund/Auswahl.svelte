<script lang="ts">
	/**
	 * Auswahl (Etappe 3a-3, Bits UI Select): eine aus mehreren Möglichkeiten.
	 * Öffnen mit Enter, Leertaste oder Pfeil, wählen mit Pfeilen und Enter,
	 * Anfangsbuchstaben springen, Escape schließt. Die Liste liegt über der
	 * Seite (Portal) und bleibt im Bildschirm.
	 *
	 *   <Auswahl beschriftung="Zeitraum" optionen={[{ wert: '7', text: '7 Tage' }]} bind:wert />
	 */
	import { Select } from 'bits-ui';
	import type { Option } from './typen.ts';

	interface Props {
		optionen: readonly Option[];
		wert?: string;
		beschriftung: string;
		platzhalter?: string;
		gesperrt?: boolean;
		onwechsel?: ((wert: string) => void) | undefined;
	}
	let {
		optionen,
		wert = $bindable(''),
		beschriftung,
		platzhalter = 'Bitte wählen',
		gesperrt = false,
		onwechsel
	}: Props = $props();
	const id = $props.id();

	const items = $derived(
		optionen.map((o) => ({ value: o.wert, label: o.text, disabled: o.gesperrt ?? false }))
	);
	const gewaehlt = $derived(optionen.find((o) => o.wert === wert)?.text ?? null);
</script>

<div class="auswahl">
	<span class="beschriftung" id="{id}-name">{beschriftung}</span>
	<Select.Root
		type="single"
		bind:value={wert}
		{items}
		disabled={gesperrt}
		onValueChange={(w: string) => onwechsel?.(w)}
	>
		<Select.Trigger aria-labelledby="{id}-name {id}-wert">
			{#snippet child({ props })}
				<button {...props} class="ausloeser">
					<span id="{id}-wert" class:leer={gewaehlt === null}>{gewaehlt ?? platzhalter}</span>
					<span class="pfeil" aria-hidden="true">▾</span>
				</button>
			{/snippet}
		</Select.Trigger>
		<Select.Portal>
			<Select.Content sideOffset={4} collisionPadding={8}>
				{#snippet child({ wrapperProps, props, open })}
					{#if open}
						<div {...wrapperProps}>
							<div {...props} class="liste">
								{#each optionen as o (o.wert)}
									<Select.Item value={o.wert} label={o.text} disabled={o.gesperrt ?? false}>
										{#snippet child({ props: eintrag, selected })}
											<div {...eintrag} class="eintrag">
												<span class="haken" aria-hidden="true">{selected ? '✓' : ''}</span>
												{o.text}
											</div>
										{/snippet}
									</Select.Item>
								{/each}
							</div>
						</div>
					{/if}
				{/snippet}
			</Select.Content>
		</Select.Portal>
	</Select.Root>
</div>

<style>
	.auswahl {
		display: grid;
		gap: 0.25rem;
		max-width: 100%;
	}
	.beschriftung {
		font-size: var(--text-sm);
		color: var(--muted);
		font-weight: 600;
	}
	.ausloeser {
		display: inline-flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		min-width: min(14rem, 100%);
		max-width: 100%;
		min-height: 2.5rem;
		padding: 0.4rem 0.75rem;
		border: 1px solid var(--muted);
		border-radius: 10px;
		background: var(--card);
		color: var(--text);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}
	.ausloeser[data-state='open'] {
		border-color: var(--accent);
	}
	.leer {
		color: var(--muted);
	}
	.pfeil {
		color: var(--muted);
	}
	.liste {
		min-width: var(--bits-select-anchor-width);
		max-width: calc(100vw - 1rem);
		max-height: min(var(--bits-select-content-available-height), 18rem);
		overflow-y: auto;
		padding: 0.25rem;
		border: 1px solid var(--line);
		border-radius: 10px;
		background: var(--card);
		color: var(--text);
		box-shadow: var(--shadow-gross);
		z-index: 50;
	}
	.eintrag {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.45rem 0.6rem;
		border-radius: 6px;
		cursor: pointer;
		overflow-wrap: anywhere;
	}
	.eintrag[data-highlighted] {
		background: var(--accent-soft);
	}
	.eintrag[data-disabled] {
		color: var(--muted);
		cursor: not-allowed;
	}
	.haken {
		display: inline-block;
		width: 1em;
		color: var(--accent-text);
	}
</style>
