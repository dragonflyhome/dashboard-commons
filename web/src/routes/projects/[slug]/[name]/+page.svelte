<script lang="ts">
	/**
	 * Unterseite eines Projekts (Etappe 3b-2) im Rahmen des Projekts.
	 */
	import { page } from '$app/state';
	import NeueFormRahmen from '#lib/components/projekt/NeueFormRahmen.svelte';
	import NeueFormUnterseite from '#lib/components/projekt/NeueFormUnterseite.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const name = $derived(page.params.name ?? '');
</script>

<NeueFormRahmen kopf={data.kopf} aktiv={name}>
	<NeueFormUnterseite kopf={data.kopf} {name} />
</NeueFormRahmen>
