<script lang="ts">
	/**
	 * Etappe 3b: Schalter für das Thema (System, Hell, Dunkel) unten auf jeder
	 * Seite. Die Wahl gilt je Browser; Lesen, Merken und Anwenden in
	 * #lib/thema.ts. Beim ersten Zeichnen hat das Skript in app.html das
	 * Attribut schon gesetzt; hier wird nur die gemerkte Wahl gezeigt.
	 */
	import { onMount } from 'svelte';
	import Wahl from '#lib/katalog/grund/Wahl.svelte';
	import {
		THEMEN,
		istThema,
		themaAnwenden,
		themaLesen,
		themaSetzen,
		type Thema
	} from '#lib/thema.ts';

	let thema = $state<Thema>('system');

	onMount(() => {
		thema = themaLesen();
		// Auch die Farbe der Titelleiste (theme-color) folgt der Wahl.
		themaAnwenden(thema);
	});
</script>

<Wahl
	bind:wert={thema}
	beschriftung="Farben"
	optionen={THEMEN}
	onwechsel={(w) => {
		if (istThema(w)) themaSetzen(w);
	}}
/>
