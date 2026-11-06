<script lang="ts">
	/**
	 * Die Projektseite; der Inhalt steht in `ProjektSeite` (auch als Teil
	 * der Gruppenseite, Entscheidung projektgruppe).
	 */
	import ProjektSeite from '#lib/components/projekt/ProjektSeite.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<ProjektSeite {data} />
