<script lang="ts">
	/**
	 * Gruppenseite `/gruppen/<slug>` (Entscheidung projektgruppe):
	 * Titel der Gruppe, oben „Wartet auf dich“ mit der Summe aller Teile und
	 * Sprungmarken zu ihnen, darunter die Teile in der Reihenfolge aus
	 * `teile`, jeder mit eigenem Kopf, eigener Fehlergrenze und eigenem
	 * Neuladen (`GruppenTeil`). Die Teile bleiben eigene Projekte mit eigenen
	 * Seiten unter `/projects/<slug>`.
	 */
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { resolve } from '$app/paths';
	import GruppenTeil from '#lib/components/projekt/GruppenTeil.svelte';
	import { anstehend } from '#lib/anstehend.svelte.ts';
	import { teilAnker, wartetSumme } from '#lib/gruppen.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const gruppe = $derived(data.gruppe);
	const summe = $derived(wartetSumme(gruppe.teile, anstehend.wartet));
	const titelVon = (slug: string): string => data.teile[slug]?.title ?? slug;
</script>

<svelte:head>
	<title>{seitenTitel(gruppe.titel)}</title>
</svelte:head>

<h1>{gruppe.titel}</h1>

<p class="wartet card" aria-label="Wartet auf dich">
	<b>Wartet auf dich:</b>
	{summe === 0 ? 'nichts' : summe === 1 ? 'ein Eintrag' : `${String(summe)} Einträge`}
	{#each gruppe.teile as t (t)}
		<span class="sprung">
			<a href="#{teilAnker(t)}">{titelVon(t)}</a>
			<span class="muted small">({anstehend.wartet[t] ?? 0})</span>
		</span>
	{/each}
	<a class="small" href="{resolve('/anstehend')}?gruppe={encodeURIComponent(gruppe.slug)}"
		>In „Anstehend“</a
	>
</p>

{#each gruppe.teile as t (t)}
	<GruppenTeil slug={t} projekt={data.teile[t] ?? null} anker={teilAnker(t)} />
{/each}

<style>
	.wartet {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.25rem 0.75rem;
		margin: 0.5rem 0 0;
		padding: 0.6rem 0.9rem;
	}
	.sprung {
		white-space: nowrap;
	}
</style>
