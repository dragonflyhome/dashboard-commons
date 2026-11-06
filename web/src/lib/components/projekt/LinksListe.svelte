<script lang="ts">
	/**
	 * Die Liste im Kasten „Links“ (alte und neue Form). Web-Adressen und Wege
	 * im Dashboard sind Links; ein lokaler Pfad (`C:\…`, `file:…`) nicht: Der
	 * Browser öffnet ihn nicht. Er steht als Text da, mit
	 * `%USERPROFILE%` statt des Kontonamens und einem Satz, dass der Chat ihn
	 * als Ort eintragen kann (Knopf „Öffnen“, Entscheidung lokal-oeffnen).
	 */
	import OldLink from '#lib/components/OldLink.svelte';
	import { SATZ_LOKALER_PFAD, istLokalerPfad, profilVerbergen } from '#lib/pfade.ts';

	interface Props {
		links: readonly { name: string; url: string }[];
	}
	let { links }: Props = $props();
</script>

<ul class="plain links">
	{#each links as l (l.url)}
		<li>
			{#if istLokalerPfad(l.url)}
				<span class="lokal">{l.name}</span>
				<span class="muted small">{profilVerbergen(l.url)}</span>
				<span class="muted small hinweis">{SATZ_LOKALER_PFAD}</span>
			{:else}
				<OldLink href={l.url} external>{l.name}</OldLink>
				<span class="muted small">{l.url}</span>
			{/if}
		</li>
	{/each}
</ul>

<style>
	.links li {
		margin: 4px 0;
	}
	.links .muted {
		overflow-wrap: anywhere;
	}
	.lokal {
		font-weight: 600;
	}
	.hinweis {
		display: block;
	}
</style>
