<script lang="ts">
	/**
	 * System-Seite /anleitung (Etappe 6b): die ganze Anleitung aus
	 * docs/anleitung/*.toml (seit 6d die einzige Anleitung), ein Abschnitt
	 * je Datei, gezeigt mit demselben Baustein wie /zielbild (`SystemAbschnitt`). Die Daten kommen geschützt über
	 * /api/anleitung; ohne Windows-Anmeldung steht nur der Weg über Edge da,
	 * nicht der Inhalt. Kein Markdown, kein HTML.
	 */
	import { resolve } from '$app/paths';
	import { Absatz, Hinweis } from '$katalog';
	import SystemAbschnitt from '#lib/components/system/SystemAbschnitt.svelte';
	import EdgeHinweis from '#lib/components/EdgeHinweis.svelte';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const abschnitte = $derived(data.anleitung?.abschnitte ?? []);
	// Wie beim Zielbild: ein Satz, wenn der Server den Ordner nicht lesen darf.
	const warnung = $derived(data.anleitung?.warnung ?? null);
</script>

<svelte:head>
	<title>{seitenTitel('Anleitung')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Anleitung</p>
<h1>Anleitung</h1>

{#if data.anmelden !== null}
	<!-- Ohne Windows-Anmeldung nur der Weg über Edge, wie bei /zielbild. -->
	<EdgeHinweis class="card muted" satz={data.anmelden} />
{:else}
	{#if data.fehler !== null}
		<Hinweis bedeutung="problem">
			Die Anleitung lässt sich gerade nicht laden: {data.fehler}
		</Hinweis>
	{/if}
	{#if warnung !== null}
		<Hinweis bedeutung="problem">{warnung}</Hinweis>
	{:else if abschnitte.length === 0 && data.fehler === null}
		<Hinweis>Die Anleitung ist gerade leer: Im Ordner docs/anleitung liegt keine Datei.</Hinweis>
	{:else if abschnitte.length > 0}
		<Absatz>
			Alles, was ein Chat und der Inhaber über das Dashboard wissen müssen: zuerst die Schritte und
			Regeln für die Chats, dann das Dashboard für den Inhaber, danach die Anleitung für die Chats
			im Einzelnen, die Technik und zuletzt die Oberfläche.
		</Absatz>
		{#each abschnitte as a (a.datei)}
			<SystemAbschnitt abschnitt={a} />
		{/each}
	{/if}
{/if}
