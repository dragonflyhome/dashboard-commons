<script lang="ts">
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import Absatz from '#lib/katalog/Absatz.svelte';
	import Liste from '#lib/katalog/Liste.svelte';
	import OldLink from '#lib/components/OldLink.svelte';
	import { zerlegen } from '#lib/hallo.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const h = $derived(data.hallo);
	const bloecke = $derived(zerlegen(h.text ?? ''));
	const datum = $derived(
		h.datum
			? new Date(`${h.datum}T12:00:00`).toLocaleDateString('de-DE', { dateStyle: 'long' })
			: null
	);
	/** „Untertitel · Datum“, was davon da ist. */
	const unterzeile = $derived([h.untertitel, datum].filter((t) => t).join(' · '));
</script>

<svelte:head>
	<title>{seitenTitel(h.titel ?? 'Hallo')}</title>
</svelte:head>

<article class="card hallo" lang={h.sprache ?? 'en'}>
	<header lang="de">
		<h1>{h.titel ?? 'Hallo'}</h1>
		{#if unterzeile}
			<p class="muted">{unterzeile}</p>
		{/if}
	</header>

	{#if !data.download}
		<div lang="de">
			<Absatz
				>Im Inhalt trägt noch kein Projekt einen Begrüßungstext (daten/eigen-hallo.toml).</Absatz
			>
		</div>
	{/if}

	{#each bloecke as b, i (i)}
		{#if b.art === 'liste'}
			<Liste punkte={b.punkte} nummeriert={b.nummeriert} />
		{:else}
			<Absatz><span class="zeilen">{b.text}</span></Absatz>
		{/if}
	{/each}

	{#if data.download}
		<footer lang="de">
			<OldLink class="button" href={data.download} download="hallo.txt"
				>Als Text herunterladen</OldLink
			>
		</footer>
	{/if}
</article>

<style>
	.hallo {
		max-width: 46rem;
	}
	.zeilen {
		white-space: pre-line;
	}
	footer {
		margin-top: 1.5rem;
	}
</style>
