<script lang="ts">
	/**
	 * System-Seite /entscheidungen (Etappe 4d): die Entscheidungen aus
	 * docs/entscheidungen, neueste zuerst. Je Eintrag Thema, Eckdaten, Urteil
	 * und „was“; alle übrigen Felder klappen auf. Die Dateien schreibt die
	 * Architektur-Rolle; die Seite zeigt nur, ohne Markdown und ohne Pfade.
	 */
	import { resolve } from '$app/paths';
	import { Absatz, Abschnitt, Aufklapper, Hinweis, Text } from '$katalog';
	import Felder from '#lib/components/system/Felder.svelte';
	import EdgeHinweis from '#lib/components/EdgeHinweis.svelte';
	import { eckdaten, filtern } from '#lib/entscheidungen.ts';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let suche = $state('');
	const alle = $derived(data.liste?.eintraege ?? []);
	// Hotfix 4d: darf der Server den Ordner nicht lesen, kommt ein Satz statt
	// einer still leeren Liste (fehlt der Ordner nur, null).
	const warnung = $derived(data.liste?.warnung ?? null);
	const gezeigt = $derived(filtern(alle, suche));
</script>

<svelte:head>
	<title>{seitenTitel('Entscheidungen')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Entscheidungen</p>
<h1>Entscheidungen</h1>
<Absatz>
	Was entschieden ist, wer es entschieden hat und was daraus folgt, die neuesten zuerst. Die
	Architektur-Rolle trägt die Entscheidungen ein.
</Absatz>

{#if data.anmelden !== null}
	<!-- Abnahme 4d: ohne Windows-Anmeldung der Weg über Edge, wie bei /anstehend. -->
	<EdgeHinweis class="card muted" satz={data.anmelden} />
{:else if data.fehler !== null}
	<Hinweis bedeutung="problem"
		>Die Entscheidungen lassen sich gerade nicht laden: {data.fehler}</Hinweis
	>
{:else if warnung !== null}
	<Hinweis bedeutung="problem">{warnung}</Hinweis>
{:else if alle.length === 0}
	<Hinweis>Hier stehen noch keine Entscheidungen.</Hinweis>
{:else}
	<div class="suche" role="search">
		<input
			type="search"
			placeholder="Suchen: Thema, Etappe, Status …"
			aria-label="Entscheidungen durchsuchen"
			bind:value={suche}
		/>
		<span class="muted small" aria-live="polite">{gezeigt.length} von {alle.length}</span>
	</div>
	{#if gezeigt.length === 0}
		<p class="muted">Keine Entscheidung passt zu „{suche}“.</p>
	{/if}
	{#each gezeigt as e (e.datei)}
		<Abschnitt titel={e.thema ?? e.datei} unterzeile={eckdaten(e) || undefined}>
			{#if e.fehler !== null}
				<Hinweis bedeutung="problem">{e.fehler}</Hinweis>
			{/if}
			{#if e.urteil !== null}
				<p class="urteil"><b>Urteil:</b> {e.urteil}</p>
			{/if}
			{#if e.was !== null}
				<Text text={e.was} />
			{/if}
			{#if e.weitere.length > 0}
				<Aufklapper titel="Alle weiteren Angaben ({e.weitere.length})">
					<Felder felder={e.weitere} />
				</Aufklapper>
			{/if}
			<p class="muted small datei">{e.datei}</p>
		</Abschnitt>
	{/each}
{/if}

<style>
	.suche {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem 1rem;
		margin: 1rem 0 0.5rem;
	}
	.suche input {
		flex: 1 1 14rem;
		min-width: 0;
		max-width: 28rem;
	}
	.urteil {
		max-width: 72ch;
	}
	.datei {
		margin-bottom: 0;
		overflow-wrap: anywhere;
	}
</style>
