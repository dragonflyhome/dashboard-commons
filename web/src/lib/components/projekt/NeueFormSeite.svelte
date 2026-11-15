<script lang="ts">
	/**
	 * Hauptseite der neuen Form: Seite.svelte des Projekts aus der Bauliste
	 * (nur Projekte mit grüner Prüfung) in einer Fehlergrenze; sonst der
	 * Standardaufbau mit dem Grund. Steht unter `/projects/<slug>`.
	 *
	 * Schritt E2b: Die Bauliste kommt dynamisch (`Bauliste`), als eigener,
	 * geschützter Teil des Baus.
	 */
	import Bauliste from '#lib/components/projekt/Bauliste.svelte';
	import type { Liste } from '#lib/components/projekt/bauliste-laden.ts';
	import Grenze from '#lib/components/projekt/Grenze.svelte';
	import Standardaufbau from '#lib/components/projekt/Standardaufbau.svelte';
	import type { ProjektAntwort } from '$vertrag';

	interface Props {
		kopf: ProjektAntwort;
	}
	let { kopf }: Props = $props();

	const slug = $derived(kopf.slug);
	// In der Liste zählen nur eigene Einträge: sonst lieferte etwa „constructor“ eine Funktion von Object.
</script>

<Bauliste datei="projects/{slug}/Seite.svelte">
	{#snippet mit(liste: Liste)}
		{@const lade = Object.hasOwn(liste.projektSeiten, slug) ? liste.projektSeiten[slug] : undefined}
		{#if lade}
			{#key slug}
				<Grenze {lade} datei="projects/{slug}/Seite.svelte" />
			{/key}
		{:else}
			<Standardaufbau {kopf} />
		{/if}
	{/snippet}
</Bauliste>
