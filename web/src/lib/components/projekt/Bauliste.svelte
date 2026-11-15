<script lang="ts">
	/**
	 * Lädt die Bauliste und zeigt dann `mit(liste)`. Bis dahin „Seite wird geladen …“.
	 * Lässt sie sich nicht laden, entscheidet `teilFehlt` (abgemeldet: zur Anmeldung;
	 * alter Bau: neu laden; abgemeldet mit offener Eingabe: `AnmeldungAbgelaufen`);
	 * sonst das Fehlerbild mit „neu versuchen“.
	 */
	import type { Snippet } from 'svelte';
	import { bauliste, type Liste } from './bauliste-laden.ts';
	import { fokusZeigen, teilFehlt } from '#lib/ladefehler.ts';
	import Fehlerbild from './Fehlerbild.svelte';
	import AnmeldungAbgelaufen from './AnmeldungAbgelaufen.svelte';

	interface Props {
		/** Pfad der Datei für die Meldung, etwa `projects/x/Seite.svelte`. */
		datei: string;
		mit: Snippet<[Liste]>;
	}
	let { datei, mit }: Props = $props();

	let versuch = $state(0);
	let liste = $state.raw<Liste | null>(null);
	let ladefehler = $state.raw<{ fehler: unknown; wie: 'abgelaufen' | 'fremd' | 'fehler' } | null>(
		null
	);
	$effect(() => {
		const nr = versuch;
		let aktuell = true;
		ladefehler = null;
		bauliste().then(
			(l) => {
				if (aktuell && nr === versuch) liste = l;
			},
			(e: unknown) => {
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
{:else if liste === null}
	<p class="muted laedt" aria-busy="true">Seite wird geladen …</p>
{:else}
	{@render mit(liste)}
{/if}

<style>
	.laedt {
		margin-top: 16px;
	}
</style>
