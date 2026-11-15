<script lang="ts">
	/**
	 * Unterseite der neuen Form: seiten/<name>.svelte aus der Bauliste, in
	 * einer Fehlergrenze. Steht unter `/projects/<slug>/<name>`.
	 *
	 * Schritt E2b: Die Bauliste kommt dynamisch (`Bauliste`), als eigener,
	 * geschützter Teil des Baus.
	 */
	import { resolve } from '$app/paths';
	import Bauliste from '#lib/components/projekt/Bauliste.svelte';
	import type { Liste } from '#lib/components/projekt/bauliste-laden.ts';
	import Grenze from '#lib/components/projekt/Grenze.svelte';
	import Standardaufbau from '#lib/components/projekt/Standardaufbau.svelte';
	import type { ProjektAntwort } from '$vertrag';

	interface Props {
		kopf: ProjektAntwort;
		name: string;
	}
	let { kopf, name }: Props = $props();

	const slug = $derived(kopf.slug);
	const seite = $derived(kopf.kopf.seiten.find((s) => s.name === name) ?? null);
	// In der Liste zählen nur eigene Einträge, nie etwas aus Object.prototype.
	const schluessel = $derived(`${slug}/${name}`);
</script>

{#if seite}
	<h2 class="titel">{seite.title}</h2>
{/if}

<Bauliste datei="projects/{slug}/seiten/{name}.svelte">
	{#snippet mit(liste: Liste)}
		{@const u = liste.projektUnterseiten}
		{@const lade = Object.hasOwn(u, schluessel) ? u[schluessel] : undefined}
		{#if lade}
			{#key schluessel}
				<Grenze {lade} datei="projects/{slug}/seiten/{name}.svelte" />
			{/key}
		{:else if seite === null}
			<section class="card fehlt">
				<h2>Diese Unterseite gibt es nicht</h2>
				<p>
					In projekt.toml steht keine Unterseite „{name}“. Unterseiten stehen dort als
					<code>[[seiten]]</code> mit <code>name</code>, <code>title</code> und <code>order</code>.
				</p>
				<p>
					<a class="button" href={resolve('/projects/[slug]', { slug })}>Zur Übersicht</a>
				</p>
			</section>
		{:else}
			<Standardaufbau {kopf} datei="seiten/{name}.svelte" />
		{/if}
	{/snippet}
</Bauliste>

<style>
	.titel {
		margin-top: 16px;
	}
	.fehlt {
		margin-top: 16px;
	}
</style>
