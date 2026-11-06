<script lang="ts">
	/**
	 * Ein hervorgehobener Hinweis; die Farbe folgt der Bedeutung. Seit 3a-3
	 * über die eine Klassenreihe b-… (klasse() in #lib/bedeutung.ts): Rand in
	 * --ton, Fläche in --ton-soft (beide aus app.css).
	 */
	import type { Snippet } from 'svelte';
	import type { Bedeutung } from '$vertrag';
	import { klasse } from '#lib/bedeutung.ts';

	interface Props {
		bedeutung?: Bedeutung | undefined;
		children: Snippet;
	}
	let { bedeutung = 'info', children }: Props = $props();
</script>

<div class="hinweis {klasse(bedeutung)}" role="note">{@render children()}</div>

<style>
	.hinweis {
		margin: 0.6em 0 1em;
		padding: 10px 14px;
		border-left: 4px solid var(--ton, var(--done));
		border-radius: 10px;
		background: var(--ton-soft, var(--done-soft));
	}
</style>
