<script lang="ts">
	/**
	 * Ein Abschnitt der Seite: Karte mit Überschrift.
	 *
	 * Zeigt der Inhalt nur Leeres (jedes Element trägt
	 * `data-leer`, etwa „Keine Fragen.“ aus `Fragen`), fehlt die ganze Karte
	 * (`hidden`); kommt Inhalt dazu, erscheint sie wieder.
	 */
	import type { Snippet } from 'svelte';
	import { nurLeeres } from './leer.ts';

	interface Props {
		titel: string;
		/** Kurzer Satz unter der Überschrift. */
		unterzeile?: string | undefined;
		children: Snippet;
	}
	let { titel, unterzeile, children }: Props = $props();
	const id = $props.id();

	let inhalt = $state<HTMLElement | null>(null);
	let leer = $state(false);
	$effect(() => {
		const el = inhalt;
		if (el === null) return;
		const pruefen = (): void => {
			leer = nurLeeres(el);
		};
		pruefen();
		const beobachter = new MutationObserver(pruefen);
		beobachter.observe(el, { childList: true, subtree: true, characterData: true });
		return () => {
			beobachter.disconnect();
		};
	});
</script>

<section class="card abschnitt" aria-labelledby="{id}-titel" hidden={leer}>
	<h2 id="{id}-titel">{titel}</h2>
	{#if unterzeile}
		<p class="muted unterzeile">{unterzeile}</p>
	{/if}
	<div class="abschnitt-inhalt" bind:this={inhalt}>
		{@render children()}
	</div>
</section>

<style>
	.abschnitt {
		margin-top: 16px;
		min-width: 0;
	}
	h2 {
		margin-top: 0;
	}
	.unterzeile {
		margin-top: -0.3em;
	}
	/* Nur zum Beobachten da: am Aufbau ändert der Ordner nichts. */
	.abschnitt-inhalt {
		display: contents;
	}
</style>
