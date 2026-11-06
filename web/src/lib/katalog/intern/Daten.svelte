<script lang="ts" generics="T extends object">
	/**
	 * Rahmen für jeden Daten-Baustein: zeigt Laden und Fehler (wenn die Quelle
	 * aus `ladeDaten` kommt), zerlegt die Datei in Teile und reicht jeden Teil
	 * mit dem passenden Abgeleiteten an den Baustein weiter.
	 */
	import type { Snippet } from 'svelte';
	import type { Abgeleitet, Datei } from '$vertrag';
	import { teile } from '#lib/daten.ts';
	import { istGeladen, type Quelle } from '../abgeleitet.ts';

	interface Props {
		daten: Quelle<T>;
		/** Nur diesen Teil zeigen (`[teil.<name>]` in der Datei). */
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
		/** Etappe B2: Satz, wenn die Daten nur mit Anmeldung kommen (sonst der allgemeine). */
		anmeldenSatz?: string | undefined;
		/** Inhalt, Name des Teils (oder `null`) und Abgeleitetes. */
		children: Snippet<[T, string | null, Abgeleitet | null]>;
	}
	let { daten, teil, abgeleitet, anmeldenSatz, children }: Props = $props();

	const geladen = $derived(istGeladen(daten) ? daten : null);
	const datei = $derived.by((): Datei<T> | null => {
		const d = daten;
		return istGeladen(d) ? d.inhalt : d;
	});
	const abg = $derived(abgeleitet ?? geladen?.abgeleitet ?? null);
	const alle = $derived(datei === null ? [] : teile(datei));
	const gewaehlt = $derived(teil === undefined ? alle : alle.filter((t) => t.name === teil));
	const benannt = $derived(teil === undefined && alle.some((t) => t.name !== null));
	let laeuft = $state(false);
	async function neu(): Promise<void> {
		if (geladen === null) return;
		laeuft = true;
		await geladen.neuLaden();
		laeuft = false;
	}
</script>

{#if geladen?.zustand === 'fehler' && geladen.ohneAnmeldung === true}
	<!-- Etappe B2: kein Fehler der Seite; ohne Anmeldung kommen diese Daten nicht. -->
	<p class="muted small anmelden">{anmeldenSatz ?? geladen.satz}</p>
{:else if geladen?.zustand === 'fehler'}
	<div class="block-error fehler" role="alert">
		<p>{geladen.satz}</p>
		<button type="button" class="button" disabled={laeuft} onclick={() => void neu()}>
			neu laden
		</button>
	</div>
{:else if datei === null}
	<p class="laedt muted small" aria-busy="true">Daten werden geladen …</p>
{:else if gewaehlt.length === 0}
	<p class="block-error">
		In der Datei gibt es keinen Teil „{teil}“.{#if alle.some((t) => t.name !== null)}
			Vorhanden: {alle.map((t) => t.name ?? '').join(', ')}.{/if}
	</p>
{:else}
	{#each gewaehlt as t (t.name ?? '')}
		{#if benannt && t.name !== null}
			<!-- h3 unter der Karte (h2); Bausteine mit eigenen Überschriften gehen darunter auf h4 (Zeitstrahl). -->
			<h3 class="teil">{t.name}</h3>
		{/if}
		{@render children(t.inhalt, t.name, abg)}
	{/each}
{/if}

<style>
	.fehler {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 8px 12px;
		margin: 0.6em 0 1em;
	}
	.fehler p {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.laedt {
		margin: 0.6em 0 1em;
		padding: 10px 12px;
		border: 1px dashed var(--line);
		border-radius: 10px;
	}
	.teil {
		font-size: var(--text-md);
		margin: 12px 0 0;
	}
</style>
