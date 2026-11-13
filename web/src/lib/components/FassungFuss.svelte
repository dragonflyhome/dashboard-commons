<script lang="ts">
	/**
	 * Fußzeile (Entscheidung „Freigabe in einem Schritt“): die laufende
	 * Fassung des Dienstes als Link auf /fassungen, zwischen zwei Fassungen
	 * „0.x.y + Unveröffentlicht“. Einmal je Laden aus `/api/betrieb` (frei);
	 * nach einer neuen Fassung lädt die Seite ohnehin neu. Antwortet der
	 * Dienst nicht, steht nur der Link ohne Nummer da.
	 *
	 * `laden` nur für Tests.
	 */
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { getJson } from '#lib/api/client.ts';
	import { fussText } from '#lib/fassungen.ts';
	import type { Betrieb } from '$vertrag';

	interface Props {
		laden?: () => Promise<Pick<Betrieb, 'version' | 'unveroeffentlicht'>>;
	}
	let { laden = () => getJson<Betrieb>('/api/betrieb') }: Props = $props();

	let text = $state('Fassungen');
	onMount(() => {
		laden().then(
			(b) => {
				text = fussText(b.version, b.unveroeffentlicht);
			},
			() => {
				// Ohne Antwort bleibt der Link ohne Nummer.
			}
		);
	});
</script>

<p><a href={resolve('/fassungen')}>{text}</a></p>
