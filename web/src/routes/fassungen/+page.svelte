<script lang="ts">
	/**
	 * Seite /fassungen (Entscheidung „Freigabe in einem Schritt“): der
	 * Änderungsverlauf aus dem Changelog, das der Dienst beim Bau einbettet.
	 * Oben die laufende Fassung, dann was seitdem unveröffentlicht dazukam,
	 * danach jede Fassung mit Datum und Zeilen, neueste zuerst. Brüche
	 * tragen das Wort hervorgehoben (`FassungZeilen`). Frei lesbar.
	 */
	import { resolve } from '$app/paths';
	import { Abschnitt, Absatz, Hinweis } from '$katalog';
	import FassungZeilen from '#lib/components/FassungZeilen.svelte';
	import { hatZeilen } from '#lib/fassungen.ts';
	import { fmtDate } from '#lib/format.ts';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const f = $derived(data.fassungen);
	const laufend = $derived(f?.fassungen.find((x) => x.version === f.laufend) ?? null);
</script>

<svelte:head>
	<title>{seitenTitel('Fassungen')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Fassungen</p>
<h1>Fassungen</h1>

{#if data.fehler !== null}
	<Hinweis bedeutung="problem">Die Fassungen lassen sich gerade nicht laden: {data.fehler}</Hinweis>
{:else if f !== null}
	<Absatz>
		Es läuft die Fassung {f.laufend}{laufend === null
			? ''
			: ` vom ${fmtDate(laufend.datum)}`}{hatZeilen(f.unveroeffentlicht)
			? ', dazu Unveröffentlichtes'
			: ''}.
	</Absatz>
	{#each f.hinweise as h, i (i)}
		<Hinweis bedeutung="problem">{h}</Hinweis>
	{/each}
	{#if hatZeilen(f.unveroeffentlicht)}
		<Abschnitt
			titel="Unveröffentlicht"
			unterzeile="Seit der letzten Fassung dazugekommen, noch nicht herausgegeben."
		>
			<FassungZeilen zeilen={f.unveroeffentlicht} />
		</Abschnitt>
	{/if}
	{#each f.fassungen as v (v.version)}
		<Abschnitt titel="{v.version} vom {fmtDate(v.datum)}">
			<FassungZeilen zeilen={v} />
		</Abschnitt>
	{/each}
{/if}
