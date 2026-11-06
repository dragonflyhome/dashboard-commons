<script lang="ts">
	/** Kennzahlen aus `daten/kennzahlen.toml`; berechnete Werte kommen fertig vom Server. */
	import type { Abgeleitet, Kennzahlen } from '$vertrag';
	import Kpi from '#lib/components/blocks/Kpi.svelte';
	import { kennzahlen } from '#lib/daten.ts';
	import Daten from './intern/Daten.svelte';
	import type { Quelle } from './abgeleitet.ts';

	interface Props {
		daten: Quelle<Kennzahlen>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
	}
	let { daten, teil, abgeleitet }: Props = $props();
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(k: Kennzahlen)}
		<Kpi items={kennzahlen(k)} />
	{/snippet}
</Daten>
