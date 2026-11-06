<script lang="ts">
	/**
	 * Knopf (Etappe 3a-3): ein Knopf für eine Handlung auf der Seite. Drei
	 * Arten: `normal`, `haupt` (die eine wichtigste Handlung, in Akzentfarbe)
	 * und `leise` (ohne Rahmen, für Nebensachen). Bits UI bringt beim Knopf
	 * nichts dazu, darum ein eigenes `<button>` mit den Klassen aus app.css.
	 * Läuft etwas, zeigt `laeuft` einen ruhigen Satz statt der Beschriftung
	 * und sperrt den Knopf (aria-busy).
	 *
	 *   <Knopf art="haupt" onclick={speichern}>Speichern</Knopf>
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		art?: 'normal' | 'haupt' | 'leise';
		type?: 'button' | 'submit';
		gesperrt?: boolean;
		/** Satz, solange etwas läuft (etwa „wird gespeichert“). */
		laeuft?: string | null;
		/** Name für Vorlese-Programme, wenn die Beschriftung allein nicht reicht. */
		name?: string | undefined;
		onclick?: ((e: MouseEvent) => void) | undefined;
		children: Snippet;
	}
	let {
		art = 'normal',
		type = 'button',
		gesperrt = false,
		laeuft = null,
		name,
		onclick,
		children
	}: Props = $props();
</script>

<button
	{type}
	class="button knopf"
	class:primary={art === 'haupt'}
	class:leise={art === 'leise'}
	disabled={gesperrt}
	aria-disabled={laeuft !== null ? true : undefined}
	aria-busy={laeuft !== null}
	aria-label={name}
	onclick={(e) => {
		// Während etwas läuft, nur gesperrt angesagt (aria-disabled), nicht
		// disabled: sonst fiele der Fokus auf body. Der Klick tut dann nichts.
		if (laeuft !== null) {
			e.preventDefault();
			return;
		}
		onclick?.(e);
	}}
>
	{#if laeuft !== null}{laeuft}{:else}{@render children()}{/if}
</button>

<style>
	.knopf {
		min-height: 2.5rem;
	}
	.leise {
		border-color: transparent;
		background: transparent;
		color: var(--accent);
	}
	/* Gesperrt zeigt der gestrichelte Rand (app.css); leise hat sonst keinen sichtbaren. */
	.leise:disabled,
	.leise[aria-disabled='true'] {
		border-color: var(--line);
	}
</style>
