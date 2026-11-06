<script lang="ts">
	/**
	 * Ein Abschnitt einer System-Seite (/zielbild, /anleitung): ein Format,
	 * ein Baustein. Reihenfolge: Satz zur Datei, Hinweise zu Blöcken („Block
	 * n: …“), `text`, `punkte` (mit `nummeriert = true` nummeriert), die Blöcke (`[[block]]`, je mit kleiner
	 * Überschrift und genau einem Inhalt), zuletzt die übrigen Felder. Alles
	 * ist reiner Text, kein Markdown, kein HTML. Tabelle und Beispiel rollen
	 * im eigenen Rahmen; die Seite wird bei 360 px nie breiter.
	 */
	import type { Snippet } from 'svelte';
	import { Abschnitt, Hinweis, Liste, Tabelle, Text } from '$katalog';
	import type { Spalte, Zeile } from '$katalog';
	import type { SystemAbschnitt, SystemBlockTabelle } from '$vertrag';
	import Beispiel from './Beispiel.svelte';
	import Felder from './Felder.svelte';

	interface Props {
		abschnitt: SystemAbschnitt;
		/** Was die Seite selbst unten anfügt (etwa ein Platzhalter). */
		children?: Snippet;
	}
	let { abschnitt: a, children }: Props = $props();
	const titel = $derived(a.titel ?? a.datei);

	function spalten(t: SystemBlockTabelle): Spalte[] {
		return t.spalten.map((s, i) => ({ schluessel: `s${String(i)}`, titel: s }));
	}
	function zeilen(t: SystemBlockTabelle): Zeile[] {
		return t.zeilen.map((z) => Object.fromEntries(z.map((w, i) => [`s${String(i)}`, w])));
	}
</script>

<Abschnitt {titel}>
	{#if a.fehler !== null}
		<Hinweis bedeutung="problem">{a.fehler}</Hinweis>
	{/if}
	{#each a.hinweise as h, i (i)}
		<Hinweis bedeutung="problem">{h}</Hinweis>
	{/each}
	{#if a.text !== null}
		<Text text={a.text} />
	{/if}
	{#if a.punkte.length > 0}
		<Liste punkte={a.punkte} nummeriert={a.nummeriert} />
	{/if}
	{#each a.bloecke as b, i (i)}
		{#if b.titel !== null}
			<h3 class="block-titel">{b.titel}</h3>
		{/if}
		{#if b.inhalt.art === 'text'}
			<Text text={b.inhalt.wert} />
		{:else if b.inhalt.art === 'punkte'}
			<Liste punkte={b.inhalt.wert} nummeriert={b.nummeriert} />
		{:else if b.inhalt.art === 'tabelle'}
			<!-- Ab etwa 10rem je Spalte rollt die Tabelle am Handy seitlich, statt Wörter zu zerreißen. -->
			<Tabelle
				beschriftung={b.titel ?? titel}
				spalten={spalten(b.inhalt.wert)}
				zeilen={zeilen(b.inhalt.wert)}
				mindestbreite={Math.min(b.inhalt.wert.spalten.length * 10, 60)}
			/>
		{:else}
			<Beispiel text={b.inhalt.wert} name={b.titel ?? `Beispiel in ${titel}`} />
		{/if}
	{/each}
	{#if a.weitere.length > 0}
		<Felder felder={a.weitere} />
	{/if}
	{@render children?.()}
</Abschnitt>

<style>
	.block-titel {
		margin: 1em 0 0.3em;
	}
</style>
