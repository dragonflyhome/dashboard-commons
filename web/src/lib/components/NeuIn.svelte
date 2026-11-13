<script lang="ts">
	/**
	 * „Neu in 0.x.y“ (Entscheidung „Freigabe in einem Schritt“): Nach einem
	 * Neustart mit neuer Fassung steht einmal oben auf der Seite, was diese
	 * Fassung bringt. Welche Fassung schon gezeigt wurde, merkt sich der
	 * Datenspeicher (`/api/fassungen/gezeigt`, geschützt): Sobald der Kasten
	 * erscheint, gilt die Fassung als gezeigt, auch in anderen Tabs und
	 * Browsern. Ohne Anmeldung fragt die Seite gar nicht (`mitZugang`) und
	 * zeigt nichts, denn merken ließe es sich nicht.
	 *
	 * `laden` und `merken` nur für Tests.
	 */
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { Knopf } from '$katalog';
	import { getJson, postJson } from '#lib/api/client.ts';
	import { mitZugang } from '#lib/zugang.ts';
	import FassungZeilen from './FassungZeilen.svelte';
	import type { FassungsEintrag, NeuIn } from '$vertrag';

	const WEG = '/api/fassungen/gezeigt';

	interface Props {
		laden?: () => Promise<NeuIn>;
		merken?: () => Promise<unknown>;
	}
	let {
		laden = () =>
			getJson<NeuIn>(
				WEG,
				mitZugang((...a) => fetch(...a))
			),
		merken = () => postJson<NeuIn>(WEG)
	}: Props = $props();

	let neu = $state<FassungsEintrag | null>(null);
	const id = $props.id();

	onMount(() => {
		laden().then(
			(a) => {
				if (a.neu_in === null) return;
				neu = a.neu_in;
				merken().catch(() => {
					// Ließ es sich nicht merken, erscheint der Kasten beim nächsten Laden noch einmal.
				});
			},
			() => {
				// Ohne Anmeldung oder Datenspeicher: kein „Neu in …“.
			}
		);
	});
</script>

{#if neu !== null}
	<section class="card neu" aria-labelledby="{id}-titel">
		<!-- Ohne Überschrift: Der Kasten steht vor der h1 jeder Seite. -->
		<p class="titel"><strong id="{id}-titel">Neu in {neu.version}</strong></p>
		<FassungZeilen zeilen={neu} ebene={null} />
		<div class="unten">
			<a href={resolve('/fassungen')}>Alle Fassungen</a>
			<Knopf
				art="leise"
				onclick={() => {
					neu = null;
					// Der Knopf verschwindet; der Fokus geht auf den Inhalt der Seite.
					document.getElementById('inhalt')?.focus();
				}}>Schließen</Knopf
			>
		</div>
	</section>
{/if}

<style>
	.neu {
		margin-bottom: 1.25rem;
	}
	.titel {
		margin-top: 0;
	}
	.unten {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
	}
</style>
