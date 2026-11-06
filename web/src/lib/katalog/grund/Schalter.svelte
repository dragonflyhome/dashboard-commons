<script lang="ts">
	/**
	 * Schalter (Etappe 3a-3, Bits UI Switch): an oder aus, sofort wirksam. Die
	 * Beschriftung steht daneben und ist mit dem Schalter verbunden; Leertaste
	 * und Enter schalten, Vorlese-Programme hören „Schalter, an/aus“.
	 *
	 *   <Schalter bind:an={nurOffene} beschriftung="Nur offene zeigen" />
	 */
	import { Switch } from 'bits-ui';

	interface Props {
		an?: boolean;
		beschriftung: string;
		/** Kurzer Satz darunter, mit dem Schalter verbunden (aria-describedby). */
		hinweis?: string | undefined;
		gesperrt?: boolean;
		onwechsel?: ((an: boolean) => void) | undefined;
	}
	let {
		an = $bindable(false),
		beschriftung,
		hinweis,
		gesperrt = false,
		onwechsel
	}: Props = $props();
	const id = $props.id();
</script>

<div class="schalter">
	<Switch.Root
		id="{id}-schalter"
		bind:checked={an}
		disabled={gesperrt}
		aria-describedby={hinweis ? `${id}-hinweis` : undefined}
		onCheckedChange={(v) => onwechsel?.(v)}
	>
		{#snippet child({ props })}
			<button {...props} class="spur">
				<Switch.Thumb>
					{#snippet child({ props: daumen })}
						<span {...daumen} class="daumen"></span>
					{/snippet}
				</Switch.Thumb>
			</button>
		{/snippet}
	</Switch.Root>
	<label for="{id}-schalter">{beschriftung}</label>
	{#if hinweis}
		<p class="hinweis muted small" id="{id}-hinweis">{hinweis}</p>
	{/if}
</div>

<style>
	.schalter {
		display: grid;
		grid-template-columns: auto 1fr;
		align-items: center;
		gap: 0.25rem 0.6rem;
	}
	.spur {
		position: relative;
		width: 2.75rem;
		height: 1.5rem;
		padding: 0;
		border: 1px solid var(--muted);
		border-radius: 999px;
		background: var(--card);
		cursor: pointer;
		transition: background 120ms ease;
	}
	.spur[data-state='checked'] {
		background: var(--accent);
		border-color: var(--accent);
	}
	/* Gesperrt: gestrichelter Rand ohne Deckkraft aus app.css; der eigene cursor hier, weil .spur ihn sonst setzt. */
	.spur:disabled {
		cursor: not-allowed;
	}
	.daumen {
		position: absolute;
		top: 50%;
		left: 0.15rem;
		width: 1.1rem;
		height: 1.1rem;
		border-radius: 50%;
		background: var(--muted);
		transform: translateY(-50%);
		transition: transform 120ms ease;
	}
	.daumen[data-state='checked'] {
		background: var(--on-accent);
		transform: translate(1.25rem, -50%);
	}
	label {
		cursor: pointer;
	}
	.hinweis {
		grid-column: 2;
		margin: 0;
	}
</style>
