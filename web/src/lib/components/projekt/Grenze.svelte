<script lang="ts">
	/**
	 * Fehlergrenze um eine Projektseite (Seite.svelte oder seiten/<name>.svelte):
	 * lädt den Teil des Baus erst beim Öffnen. Wirft die Seite beim Zeichnen
	 * einen Fehler, zeigt die Grenze nur hier „Diese Seite hat gerade einen
	 * Fehler“ mit „neu versuchen“ (Etappe 4b, `Fehlerbild`); Kopf, Navigation
	 * und der Rest des Dashboards laufen weiter. Fehler in Klick-Funktionen
	 * fängt die Grenze nicht (so die Svelte-Doku); die fängt der Rahmen
	 * (`NeueFormRahmen`) mit `onerror` am Fenster.
	 *
	 * Fehlt nur ein Teil eines alten Baus (nach einer neuen Fassung, 404),
	 * lädt die Seite einmal neu statt des Fehlerbilds (`#lib/fassung.svelte.ts`).
	 */
	import type { Component } from 'svelte';
	import { istChunkFehler } from '#lib/fassung.ts';
	import { fassung } from '#lib/fassung.svelte.ts';
	import Fehlerbild from './Fehlerbild.svelte';

	interface Props {
		lade: () => Promise<{ default: Component }>;
		/** Pfad der Datei für die Meldung, etwa `projects/x/Seite.svelte`. */
		datei: string;
	}
	let { lade, datei }: Props = $props();

	/** Zählt „neu versuchen“ nach einem Ladefehler: lädt dann noch einmal. */
	let versuch = $state(0);
	/** Die geladene Seite; erst danach zeichnet die Grenze sie (synchron, damit sie Fehler fängt). */
	let Seite = $state.raw<Component | null>(null);
	let ladefehler = $state.raw<{ fehler: unknown } | null>(null);
	$effect(() => {
		const laden = lade;
		// Gelesen, damit „neu versuchen“ (versuch + 1) noch einmal lädt.
		const nr = versuch;
		let aktuell = true;
		Seite = null;
		ladefehler = null;
		laden().then(
			(m) => {
				if (aktuell && nr === versuch) Seite = m.default;
			},
			(e: unknown) => {
				// Alter Bau: neu laden; bis dahin bleibt „Seite wird geladen …“ stehen.
				if (aktuell && istChunkFehler(e) && fassung.chunkFehler()) return;
				if (aktuell) ladefehler = { fehler: e };
			}
		);
		return () => {
			aktuell = false;
		};
	});

	function melden(e: unknown): void {
		console.error(`Fehler in ${datei}:`, e);
	}
</script>

{#if ladefehler !== null}
	<Fehlerbild
		satz="Die Seite ließ sich nicht laden."
		fehler={ladefehler.fehler}
		{datei}
		neu={() => {
			versuch += 1;
		}}
	/>
{:else if Seite === null}
	<p class="muted laedt" aria-busy="true">Seite wird geladen …</p>
{:else}
	<svelte:boundary onerror={melden}>
		<Seite />

		{#snippet failed(error: unknown, reset: () => void)}
			<Fehlerbild
				satz="Beim Zeichnen ist etwas schiefgegangen; der Chat des Projekts kann es beheben."
				fehler={error}
				{datei}
				neu={reset}
			/>
		{/snippet}
	</svelte:boundary>
{/if}

<style>
	.laedt {
		margin-top: 16px;
	}
</style>
