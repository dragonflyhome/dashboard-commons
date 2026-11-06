<script lang="ts">
	/**
	 * Die Archivansicht (Entscheidung archiv-status): archivierte
	 * Projekte mit Titel und seit wann. Der einzige Weg hierher ist der
	 * schlichte Link „Archiv“ am Ende der Projektübersicht auf der
	 * Startseite; die Seitenleiste nennt diese Seite nicht. Jedes Projekt führt
	 * zu seiner Seite, die oben sagt, dass es archiviert ist.
	 */
	import { resolve } from '$app/paths';
	import { Hinweis } from '$katalog';
	import { fmtDate } from '#lib/format.ts';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const projekte = $derived(data.liste?.projekte ?? []);
</script>

<svelte:head>
	<title>{seitenTitel('Archiv')}</title>
</svelte:head>

<h1>Archiv</h1>
<p class="muted">Ruhende Projekte. Sie melden sich nirgends; ihre Seiten bleiben, wie sie sind.</p>

{#if data.fehler !== null}
	<Hinweis bedeutung="problem">Das Archiv lässt sich gerade nicht laden: {data.fehler}</Hinweis>
{:else if projekte.length === 0}
	<p class="muted">Im Archiv liegt nichts.</p>
{:else}
	<ul class="plain archiv">
		{#each projekte as p (p.slug)}
			<li class="card">
				<a href={resolve('/projects/[slug]', { slug: p.slug })}>{p.titel}</a>
				{#if p.seit !== null}
					<span class="muted small">archiviert seit {fmtDate(p.seit)}</span>
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	.archiv {
		display: grid;
		gap: 0.5rem;
	}
	.archiv li {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.3rem 1rem;
	}
</style>
