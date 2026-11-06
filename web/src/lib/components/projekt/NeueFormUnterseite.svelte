<script lang="ts">
	/**
	 * Unterseite der neuen Form: seiten/<name>.svelte aus der Bauliste, in
	 * einer Fehlergrenze. Steht unter `/projects/<slug>/<name>`.
	 */
	import { resolve } from '$app/paths';
	import { projektUnterseiten } from '#lib/projekte.generated.ts';
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
	// Nur eigene Einträge, nie etwas aus Object.prototype.
	const schluessel = $derived(`${slug}/${name}`);
	const lade = $derived(
		Object.hasOwn(projektUnterseiten, schluessel) ? projektUnterseiten[schluessel] : undefined
	);
</script>

{#if seite}
	<h2 class="titel">{seite.title}</h2>
{/if}

{#if lade}
	{#key `${slug}/${name}`}
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

<style>
	.titel {
		margin-top: 16px;
	}
	.fehlt {
		margin-top: 16px;
	}
</style>
