<script lang="ts">
	/**
	 * „Alle Fristen“ unten in „Anstehend“ (Etappe B3, Modul Fristen): die
	 * Fristenliste über alle Projekte (`GET /api/fristen`, geschützt), mit dem
	 * Baustein `Fristenliste` aus `$katalog`. Warum hier und nicht auf einer
	 * eigenen Seite: Anstehend ist der eine Ort, an dem der Inhaber sieht, was
	 * kommt; eine eigene Seite bräuchte einen Weg in die Kopfleiste und zeigte
	 * dieselben Aufgaben ein zweites Mal ohne Antwortknöpfe. Archivierte
	 * Projekte fehlen; für sie steht oben die neutrale Zeile.
	 *
	 * Zugeklappt, damit die Liste der Einträge oben bleibt; lädt neu bei
	 * `change` für Anstehend oder Aufgaben, nach dem Nachholen und um Mitternacht.
	 */
	import type { FristenListe } from '$vertrag';
	import { Fristenliste } from '$katalog';
	import { ladeFristen } from '#lib/fristen.ts';
	import { live } from '#lib/live.svelte.ts';
	import { beiTageswechsel } from '#lib/tageswechsel.ts';
	import { mitZugang } from '#lib/zugang.ts';

	interface Props {
		/** Für Tests; sonst das globale fetch. */
		fetchFn?: typeof fetch;
	}
	let { fetchFn = (...a) => fetch(...a) }: Props = $props();

	let liste = $state<FristenListe | null>(null);
	let anfrage = 0;

	async function laden(): Promise<void> {
		const nr = ++anfrage;
		const l = await ladeFristen(mitZugang(fetchFn));
		if (nr === anfrage) liste = l;
	}

	$effect(() => {
		void laden();
		const aus = [
			live.onChange((c) => {
				if (c.area === 'anstehend' || c.area === 'aufgaben') void laden();
			}),
			live.onNachholen(() => {
				void laden();
			}),
			beiTageswechsel(() => {
				void laden();
			})
		];
		return () => {
			for (const ab of aus) ab();
		};
	});
</script>

{#if liste !== null}
	<details class="card alle-fristen">
		<summary>
			<h2>Alle Fristen <span class="anzahl">{liste.zeilen.length}</span></h2>
		</summary>
		<p class="muted small">
			Aus Aufgaben, Fragen, Arbeiten und den Fristen der Projekte, nach Datum; bewegliche Fristen
			ohne Datum am Ende.
		</p>
		<Fristenliste {liste} mitProjekt leerSatz="Gerade läuft keine Frist." />
	</details>
{/if}

<style>
	.alle-fristen {
		margin: 1rem 0;
	}
	summary {
		cursor: pointer;
	}
	summary h2 {
		display: inline;
		font-size: var(--text-md);
	}
	.anzahl {
		color: var(--muted);
		font-weight: 400;
	}
</style>
