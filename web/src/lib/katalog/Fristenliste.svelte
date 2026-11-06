<script lang="ts">
	/**
	 * Fristenliste (Etappe B3, Modul Fristen): alle offenen Fristen eines
	 * Projekts nach Datum, die überfälligen zuerst; bewegliche Fristen, deren
	 * Ereignis noch kein Datum hat, am Ende mit „hängt an …“. Quellen sind
	 * `daten/fristen.toml`, die Aufgaben, die sichtbare Frage und die Arbeiten
	 * mit Frist; nie der Zeitstrahl. Alles rechnet der Server
	 * (`abgeleitet.fristen` zu `ladeDaten('fristen')`, übergreifend
	 * `GET /api/fristen`); der Baustein zeigt nur.
	 *
	 * In `Seite.svelte`: `<Fristenliste daten={ladeDaten('fristen')} />`. Mit
	 * `liste` lässt er sich mit fertigen Daten zeigen (Katalog, „Anstehend“),
	 * mit `mitProjekt` je Zeile mit dem Projekt. Fristtitel sind persönlich:
	 * Ohne Anmeldung steht nur der Satz zum Anmelden da.
	 */
	import type { Abgeleitet, FristenListe, Fristen, FristZeile } from '$vertrag';
	import { istGeladen, type Quelle } from './abgeleitet.ts';
	import { BROWSER_ADRESSE } from '#lib/anmeldung.ts';
	import { fristDringend, fristWorte } from '#lib/anstehend.ts';
	import { bezugSatz, haengtSatz, QUELLE_WORT } from '#lib/fristen.ts';
	import { fmtDate } from '#lib/format.ts';
	import { zielHref } from '#lib/zieladresse.ts';

	interface Props {
		/** Die Datei aus `ladeDaten('fristen')`; die Liste steht in ihrem Abgeleiteten. */
		daten?: Quelle<Fristen> | undefined;
		/** Oder gleich die Liste (übergreifend oder als Beispiel). */
		liste?: FristenListe | null | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
		/** Je Zeile das Projekt nennen (übergreifende Liste). */
		mitProjekt?: boolean;
		/** Satz, wenn nichts offen ist. */
		leerSatz?: string;
	}
	let {
		daten,
		liste,
		abgeleitet,
		mitProjekt = false,
		leerSatz = 'Keine offenen Fristen.'
	}: Props = $props();

	const geladen = $derived(daten !== undefined && istGeladen(daten) ? daten : null);
	const fristen = $derived(liste ?? (abgeleitet ?? geladen?.abgeleitet ?? null)?.fristen ?? null);

	/** Anker nur für Fristen aus fristen.toml (Ziel `#frist-<kennung>`); Einträge haben ihren eigenen. */
	function anker(z: FristZeile): string | undefined {
		return z.quelle === 'datei' && !mitProjekt ? `frist-${z.id}` : undefined;
	}
</script>

{#snippet zeile(z: FristZeile)}
	{@const href = zielHref(z.ziel)}
	{@const haengt = haengtSatz(z)}
	{@const aus = bezugSatz(z)}
	{@const worte = fristWorte(z.tage_bis)}
	<li id={anker(z)} class:haengt={haengt !== null} data-frist-stufe={z.frist_stufe ?? 'keine'}>
		<span class="wann" class:achtung={fristDringend(z.frist_stufe)}>
			{#if z.frist !== null}
				<b>{fmtDate(z.frist)}</b>{#if worte}<span class="small">, {worte}</span>{/if}
			{:else}
				<span class="muted small">ohne Datum</span>
			{/if}
		</span>
		<span class="was">
			{#if href !== null && anker(z) === undefined}
				<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- zielHref baut mit resolve() -->
				<a {href}>{z.titel}</a>
			{:else}
				{z.titel}
			{/if}
			<span class="meta muted small">
				{QUELLE_WORT[z.quelle]}{#if mitProjekt && z.projekt_titel !== null}
					· {z.projekt_titel}{/if}{#if haengt !== null}
					· {haengt}{:else if aus !== null}
					· {aus}{/if}{#if z.hinweis}
					· {z.hinweis}{/if}
			</span>
		</span>
	</li>
{/snippet}

{#if geladen?.zustand === 'fehler' && geladen.ohneAnmeldung === true}
	<p class="muted small">
		Anmelden, um die Fristen zu sehen: in Microsoft Edge unter {BROWSER_ADRESSE}.
	</p>
{:else if geladen?.zustand === 'fehler'}
	<p class="block-error" role="alert">{geladen.satz}</p>
{:else if fristen === null}
	<p class="muted small" aria-busy="true">Fristen werden geladen …</p>
{:else}
	{#each fristen.hinweise as h (h)}
		<p class="block-error">{h}</p>
	{/each}
	{#if fristen.speicher_fehlt}
		<p class="muted small">
			Der Datenspeicher fehlt gerade: Fristen von Aufgaben und Arbeiten fehlen.
		</p>
	{/if}
	{#if fristen.zeilen.length === 0}
		<p class="muted small" data-leer>{leerSatz}</p>
	{:else}
		<ul class="plain fristenliste">
			{#each fristen.zeilen as z (`${z.projekt}-${z.quelle}-${z.id}`)}
				{@render zeile(z)}
			{/each}
		</ul>
	{/if}
{/if}

<style>
	.fristenliste {
		display: grid;
		gap: 0.35rem;
		margin: 0.4em 0 0.8em;
	}
	li {
		display: grid;
		grid-template-columns: minmax(8rem, 12rem) 1fr;
		gap: 0.2rem 0.75rem;
		padding: 0.35rem 0;
		border-bottom: 1px solid var(--line);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	li:last-child {
		border-bottom: 0;
	}
	.was {
		display: grid;
		gap: 0.1rem;
		min-width: 0;
	}
	.achtung {
		color: var(--warn-text);
		font-weight: 600;
	}
	@media (max-width: 37.5rem) {
		li {
			grid-template-columns: 1fr;
		}
	}
</style>
