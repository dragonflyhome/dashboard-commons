<script lang="ts">
	/**
	 * System-Seite /zielbild (Etappe 4d): nur der Rahmen. Die Abschnitte legt
	 * die Architektur-Rolle als docs/zielbild/*.toml an (Form in der Anleitung,
	 * „System-Seiten“), mit Blöcken für Tabellen und Beispiele; jeder Abschnitt
	 * ist ein SystemAbschnitt wie auf /anleitung. Ohne Abschnitte steht hier
	 * ein kurzer Satz.
	 */
	import { resolve } from '$app/paths';
	import { Hinweis } from '$katalog';
	import SystemAbschnitt from '#lib/components/system/SystemAbschnitt.svelte';
	import EdgeHinweis from '#lib/components/EdgeHinweis.svelte';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const abschnitte = $derived(data.zielbild?.abschnitte ?? []);
	// Hotfix 4d: Satz, wenn der Server den Ordner nicht lesen darf (statt Platzhalter).
	const warnung = $derived(data.zielbild?.warnung ?? null);
</script>

<svelte:head>
	<title>{seitenTitel('Zielbild')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Zielbild</p>
<h1>Zielbild</h1>

{#if data.anmelden !== null}
	<!-- Abnahme 4d: ohne Windows-Anmeldung nur der Weg über Edge, wie bei /anstehend. -->
	<EdgeHinweis class="card muted" satz={data.anmelden} />
{:else}
	{#if data.fehler !== null}
		<Hinweis bedeutung="problem">Das Zielbild lässt sich gerade nicht laden: {data.fehler}</Hinweis>
	{/if}
	{#if warnung !== null}
		<Hinweis bedeutung="problem">{warnung}</Hinweis>
	{:else if abschnitte.length === 0}
		<Hinweis>Im Zielbild steht noch kein Abschnitt.</Hinweis>
	{:else}
		{#each abschnitte as a (a.datei)}
			<SystemAbschnitt abschnitt={a} />
		{/each}
	{/if}
{/if}
