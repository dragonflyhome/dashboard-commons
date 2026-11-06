<script lang="ts">
	/**
	 * Hauptseite der neuen Form: Seite.svelte des Projekts aus der Bauliste
	 * (nur Projekte mit grüner Prüfung) in einer Fehlergrenze; sonst der
	 * Standardaufbau mit dem Grund. Steht unter `/projects/<slug>`.
	 */
	import { projektSeiten } from '#lib/projekte.generated.ts';
	import Grenze from '#lib/components/projekt/Grenze.svelte';
	import Standardaufbau from '#lib/components/projekt/Standardaufbau.svelte';
	import type { ProjektAntwort } from '$vertrag';

	interface Props {
		kopf: ProjektAntwort;
	}
	let { kopf }: Props = $props();

	const slug = $derived(kopf.slug);
	// Nur eigene Einträge: sonst lieferte etwa „constructor“ eine Funktion von Object.
	const lade = $derived(Object.hasOwn(projektSeiten, slug) ? projektSeiten[slug] : undefined);
</script>

{#if lade}
	{#key slug}
		<Grenze {lade} datei="projects/{slug}/Seite.svelte" />
	{/key}
{:else}
	<Standardaufbau {kopf} />
{/if}
