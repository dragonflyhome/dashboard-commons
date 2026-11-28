<script lang="ts">
	/**
	 * Der Himmel oben rechts (Kalender, Stufe 6, Blickort seit 7b): die Mondphase
	 * (`kalender/Mond.svelte`, hier nur Schmuck), Sonnenauf- und -untergang von heute
	 * und der Blickort; ein Link zu /heute. In sich geschlossen: keine Eigenschaften
	 * vom Layout; er meldet sich in `onMount` beim gemeinsamen Laden
	 * (`himmel.svelte.ts`) an und beim Entfernen ab.
	 * Ohne 200-Antwort (ohne Anmeldung, fremdes Konto, Fehler, ohne Netz) zeichnet
	 * er gar nichts. Die Breitenregeln stehen nur hier: unter 64rem (Handy,
	 * Tablet im Hochformat) nur das Symbol.
	 */
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { himmel, himmelSatz } from '#lib/himmel.svelte.ts';
	import Mond from '../kalender/Mond.svelte';

	onMount(() => himmel.nutzen());

	const k = $derived(himmel.kopf);
	const hier = $derived(page.route.id === '/heute');
</script>

{#if k !== null}
	{@const satz = himmelSatz(k)}
	{@const zeiten = k.aufgang !== null || k.untergang !== null}
	<a
		class="himmel"
		href={resolve('/heute')}
		aria-label={satz}
		title={satz}
		aria-current={hier ? 'page' : undefined}
		data-himmel
	>
		<Mond kopf={k} />
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
		color: var(--vordergrund-leise);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		text-decoration: none;
	}
	.himmel:hover {
		background: var(--grundfarbe-soft);
		color: var(--vordergrund);
	}
	.himmel[aria-current='page'] {
		background: var(--grundfarbe-soft);
		color: var(--grundfarbe-text);
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
