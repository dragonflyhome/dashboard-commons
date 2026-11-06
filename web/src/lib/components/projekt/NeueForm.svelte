<script lang="ts">
	/**
	 * Projektseite in der neuen Form: je Baustein aus `daten/*.toml` eine
	 * Karte, darunter eigene Daten und die geplanten Unterseiten.
	 */
	import type { ProjektDaten, Seite } from '$vertrag';
	import { karten } from '#lib/daten.ts';
	import BlockAnzeige from '../BlockAnzeige.svelte';
	import FragenListe from './FragenListe.svelte';
	import Progress from '../blocks/Progress.svelte';
	import StandardDatei from './StandardDatei.svelte';
	import { setzeProjekt } from '#lib/projekt/laden.svelte.ts';

	interface Props {
		/** Für den Zeitstrahl: seine Ereignisse rechnet der Server je Datei (ladeDaten). */
		slug: string;
		daten: ProjektDaten | null;
		seiten: Seite[];
	}
	let { slug, daten, seiten }: Props = $props();
	setzeProjekt(() => slug);

	const liste = $derived(daten === null ? [] : karten(daten));
	const eigen = $derived(daten === null ? [] : Object.entries(daten.eigen));
	const unterseiten = $derived([...seiten].sort((a, b) => a.order - b.order));
</script>

{#if liste.length === 0 && eigen.length === 0}
	<section class="card">
		<p class="muted">Noch keine Daten in <code>daten/</code>.</p>
	</section>
{/if}

{#each liste as k (k.schluessel)}
	<section class="card" aria-labelledby="baustein-{k.schluessel}">
		<h2 id="baustein-{k.schluessel}">{k.titel}</h2>
		{#if k.schluessel === 'zeitstrahl'}
			<!-- Etappe 3a-1: der Verlauf braucht abgeleitet.ereignisse; die bringt ladeDaten mit. -->
			<StandardDatei name="zeitstrahl" />
		{:else if k.schluessel === 'orte'}
			<!-- Entscheidung lokal-oeffnen: Knopf „Öffnen“ je Ort, nie ein Pfad. -->
			<StandardDatei name="orte" />
		{:else if k.schluessel === 'fristen'}
			<!-- Etappe B3: die Fristenliste aus abgeleitet.fristen (nur mit Anmeldung). -->
			<StandardDatei name="fristen" />
		{:else}
			{#each k.teile as t, i (t.name ?? i)}
				{#if t.name !== null}
					<h3>{t.name}</h3>
				{/if}
				{#if t.anzeige.art === 'fragen'}
					<FragenListe fragen={t.anzeige.fragen} />
				{:else if t.anzeige.art === 'fortschritt'}
					<Progress rows={t.anzeige.zeilen} />
				{:else}
					<BlockAnzeige block={t.anzeige.block} />
				{/if}
			{/each}
		{/if}
	</section>
{/each}

{#if eigen.length > 0}
	<section class="card" aria-labelledby="eigene-daten">
		<h2 id="eigene-daten">Eigene Daten</h2>
		{#each eigen as [name, wert] (name)}
			<details>
				<summary><code>eigen-{name}.toml</code></summary>
				<pre class="json">{JSON.stringify(wert, null, 2)}</pre>
			</details>
		{/each}
	</section>
{/if}

{#if unterseiten.length > 0}
	<section class="card" aria-labelledby="unterseiten">
		<h2 id="unterseiten">Unterseiten</h2>
		<ul class="plain">
			{#each unterseiten as s (s.name)}
				<li>{s.title}</li>
			{/each}
		</ul>
		<p class="muted small">Die Unterseiten selbst kommen mit Etappe 4.</p>
	</section>
{/if}

<style>
	section {
		margin-top: 16px;
	}
	h3 {
		font-size: var(--text-md);
		margin: 12px 0 0;
	}
	summary {
		cursor: pointer;
	}
	.json {
		font-family: var(--mono);
		font-size: var(--text-sm);
		overflow: auto;
		max-height: 360px;
		padding: 8px 10px;
		border: 1px solid var(--line);
		border-radius: 8px;
	}
</style>
