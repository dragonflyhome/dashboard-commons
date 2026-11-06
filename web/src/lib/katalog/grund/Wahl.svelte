<script lang="ts">
	/**
	 * Wahl (Etappe 3b, Bits UI RadioGroup): eine aus wenigen Möglichkeiten,
	 * nebeneinander wie Segmente, sofort wirksam. Tab geht auf die gewählte,
	 * die Pfeile wechseln (und wählen gleich); Vorlese-Programme hören
	 * „Optionsfeld, ausgewählt, 1 von 3“ mit dem Namen der Gruppe. Jede
	 * Möglichkeit ist mindestens 44 px hoch (Ziel für Finger).
	 *
	 *   <Wahl bind:wert={ansicht} beschriftung="Ansicht"
	 *     optionen={[{ wert: 'kacheln', text: 'Kacheln' }, { wert: 'spalten', text: 'Spalten' }]} />
	 */
	import { RadioGroup } from 'bits-ui';
	import type { Option } from './typen.ts';

	interface Props {
		wert?: string;
		/** Name der Gruppe; steht davor, mit `beschriftungZeigen={false}` nur für Vorlese-Programme. */
		beschriftung: string;
		beschriftungZeigen?: boolean;
		optionen: readonly Option[];
		onwechsel?: ((wert: string) => void) | undefined;
	}
	let {
		wert = $bindable(''),
		beschriftung,
		beschriftungZeigen = true,
		optionen,
		onwechsel
	}: Props = $props();
	const id = $props.id();
</script>

<div class="wahl">
	<span id="{id}-name" class="name" class:nur-vorlesen={!beschriftungZeigen}>{beschriftung}</span>
	<RadioGroup.Root
		bind:value={wert}
		orientation="horizontal"
		aria-labelledby="{id}-name"
		onValueChange={(v) => onwechsel?.(v)}
	>
		{#snippet child({ props })}
			<div {...props} class="segmente">
				{#each optionen as o (o.wert)}
					<RadioGroup.Item value={o.wert} disabled={o.gesperrt ?? false}>
						{#snippet child({ props: knopf })}
							<button {...knopf} class="segment">{o.text}</button>
						{/snippet}
					</RadioGroup.Item>
				{/each}
			</div>
		{/snippet}
	</RadioGroup.Root>
</div>

<style>
	.wahl {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.25rem 0.6rem;
	}
	.name {
		color: var(--muted);
		font-size: var(--text-sm);
	}
	.segmente {
		display: inline-flex;
		padding: 0.125rem;
		gap: 0.125rem;
		border: 1px solid var(--line);
		border-radius: 999px;
		background: var(--card);
	}
	.segment {
		min-height: 2.75rem;
		min-width: 2.75rem;
		padding: 0 0.9rem;
		border: 0;
		border-radius: 999px;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--text-sm);
		cursor: pointer;
	}
	.segment:hover {
		background: var(--accent-soft);
	}
	.segment[data-state='checked'] {
		background: var(--accent);
		color: var(--on-accent);
		font-weight: 600;
	}
	/* Gesperrt regelt app.css (gestrichelter Rand, ohne Deckkraft); das Segment hat
	   keinen eigenen Rand, darum hier die gestrichelte Linie innen. */
	.segment:disabled {
		cursor: not-allowed;
		outline: 1px dashed var(--muted);
		outline-offset: -0.2rem;
	}
</style>
