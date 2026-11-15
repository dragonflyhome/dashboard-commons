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
	 * Schritt E2b: Gilt die Anmeldung nicht mehr (401 auf den geschützten Teil),
	 * geht es stattdessen zur Anmeldung (`#lib/ladefehler.ts`); ist dabei etwas
	 * Ungesichertes offen, bleibt die Seite mit dem Hinweis `AnmeldungAbgelaufen`.
	 */
	import type { Component } from 'svelte';
	import { fokusZeigen, teilFehlt } from '#lib/ladefehler.ts';
	import Fehlerbild from './Fehlerbild.svelte';
	import AnmeldungAbgelaufen from './AnmeldungAbgelaufen.svelte';

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
	let ladefehler = $state.raw<{ fehler: unknown; wie: 'abgelaufen' | 'fremd' | 'fehler' } | null>(
		null
	);
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
				// Abgemeldet: zur Anmeldung; alter Bau: neu laden. Bis dahin bleibt
				// „Seite wird geladen …“ stehen.
				void teilFehlt(e).then((wie) => {
					if (aktuell && nr === versuch && wie !== 'weiter') {
						ladefehler = { fehler: e, wie };
						void fokusZeigen();
					}
				});
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

{#if ladefehler !== null && ladefehler.wie !== 'fehler'}
	<AnmeldungAbgelaufen
		fremd={ladefehler.wie === 'fremd'}
		satz="Dieser Teil der Seite lässt sich ohne Anmeldung nicht laden; was sonst auf der Seite eingegeben ist, bleibt stehen. Bitte in einem neuen Tab anmelden und dann neu versuchen."
		knopf="neu versuchen"
		neu={() => {
			versuch += 1;
		}}
	/>
{:else if ladefehler !== null}
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
