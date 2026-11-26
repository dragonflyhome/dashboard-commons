<script lang="ts">
	/**
	 * Der Himmel oben rechts (Kalender, Stufe 6): die Mondphase als selbst
	 * gezeichnetes Symbol, Sonnenauf- und -untergang von heute und der Heimatort;
	 * ein Link zu /kalender. In sich geschlossen: keine Eigenschaften vom Layout,
	 * der Laden (`himmel.svelte.ts`) startet in `onMount` und endet beim Entfernen.
	 * Ohne 200-Antwort (ohne Anmeldung, fremdes Konto, Fehler, ohne Netz) zeichnet
	 * er gar nichts. Die Breitenregeln stehen nur hier: unter 64rem (Handy,
	 * Tablet im Hochformat) nur das Symbol.
	 */
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { HimmelLaden, himmelSatz, mondPfad } from '#lib/himmel.svelte.ts';

	const himmel = new HimmelLaden();
	onMount(() => himmel.start());

	const k = $derived(himmel.kopf);
	const hier = $derived(page.route.id === '/kalender');
</script>

{#if k !== null}
	{@const satz = himmelSatz(k)}
	{@const mond = mondPfad(k.phase_grad, 9)}
	{@const zeiten = k.aufgang !== null || k.untergang !== null}
	<a
		class="himmel"
		href={resolve('/kalender')}
		aria-label={satz}
		title={satz}
		aria-current={hier ? 'page' : undefined}
		data-himmel
	>
		<svg viewBox="-10 -10 20 20" width="22" height="22" aria-hidden="true">
			<circle class="dunkel" r="9" />
			<path
				class="hell"
				d={mond.d}
				transform={mond.spiegeln !== k.sued ? 'scale(-1 1)' : undefined}
			/>
		</svg>
		{#if zeiten}
			<span class="zeiten" aria-hidden="true"
				>{#if k.aufgang !== null}↑ {k.aufgang}{/if}
				{#if k.untergang !== null}↓ {k.untergang}{/if}</span
			>
		{/if}
		<span class="ort" aria-hidden="true">{zeiten ? '· ' : ''}{k.ort}</span>
	</a>
{/if}

<style>
	.himmel {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.375rem;
		min-width: 2.75rem;
		min-height: 2.75rem;
		padding: 0 0.375rem;
		border-radius: 0.625rem;
		color: var(--muted);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		text-decoration: none;
	}
	.himmel:hover {
		background: var(--accent-soft);
		color: var(--text);
	}
	.himmel[aria-current='page'] {
		background: var(--accent-soft);
		color: var(--accent-text);
	}
	svg {
		flex: none;
	}
	/* Der Rand hält auch den Neumond sichtbar (gegen --card und --accent-soft über 3:1). */
	.dunkel {
		fill: var(--line);
		stroke: var(--muted);
		stroke-width: 1;
	}
	.hell {
		fill: var(--text);
	}
	/* Den Ortsnamen erst breit ganz, sonst gekürzt (der volle steht im Namen des Links). */
	.ort {
		max-width: 8rem;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	@media (min-width: 80rem) {
		.ort {
			max-width: 18rem;
		}
	}
	/*
	 * Solange die Leiste als Schublade kommt (unter 64rem, Handy und Tablet im
	 * Hochformat), gibt der Baustein zuerst nach: nur das Symbol. Mit Zeiten (gemessen
	 * 123 px) laufen die Brotkrumen einer Unterseite schon bei 768 px in ihn hinein.
	 * Der volle Satz bleibt im Namen des Links.
	 */
	@media (max-width: 63.99rem) {
		.himmel {
			padding: 0;
		}
		.zeiten,
		.ort {
			display: none;
		}
	}
</style>
