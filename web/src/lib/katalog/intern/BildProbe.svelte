<script lang="ts">
	/**
	 * Nur für Tests (Bild.svelte.spec.ts): setzt den Projekt-Kontext wie die
	 * Projektseite unter `/projects/<slug>` und zeigt darin ein `Bild`.
	 */
	import type { ComponentProps } from 'svelte';
	import { setzeProjekt } from '../../projekt/laden.svelte.ts';
	import Bild from '../Bild.svelte';

	type Props = { slug: string } & ComponentProps<typeof Bild>;
	let { slug, ...bild }: Props = $props();
	setzeProjekt(() => slug);
</script>

<Bild {...bild} />
