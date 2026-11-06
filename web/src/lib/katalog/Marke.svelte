<script lang="ts">
	/**
	 * Marke (Etappe B1): ein Stand als kurzes Wort mit Bedeutung, etwa
	 * „bezahlt“ (ok), „wartet“ (wartet) oder „fehlt“ (problem). Für Zellen
	 * einer Tabelle und Einträge in Listen; ersetzt eigene Stile wie
	 * `.marke-…` in Projekten.
	 *
	 * Die Farbe kommt aus der Klasse der Bedeutung (bedeutung.ts, app.css:
	 * Fläche `--ton-soft`, Punkt `--ton`); das Wort steht immer dabei, die
	 * Farbe nie allein. Welche Bedeutung ein Wort aus TOML hat, sagt der
	 * Server (`abgeleitet.listen.<liste>.marken`, `farbe` in den Daten); die
	 * Oberfläche übersetzt keine Wörter.
	 *
	 *   <Marke bedeutung="ok">bezahlt</Marke>
	 *   <Marke bedeutung="wartet" wort="offen" />
	 */
	import type { Snippet } from 'svelte';
	import type { Bedeutung } from '$vertrag';
	import { klasse, wort as bedeutungsWort } from '#lib/bedeutung.ts';

	interface Props {
		bedeutung: Bedeutung;
		/** Das Wort; ohne `wort` und ohne Inhalt das Wort der Bedeutung („in Ordnung“). */
		wort?: string | undefined;
		children?: Snippet;
	}
	let { bedeutung, wort, children }: Props = $props();
</script>

<span class="marke {klasse(bedeutung)}" data-bedeutung={bedeutung}
	><span class="punkt" aria-hidden="true"></span>{#if children}{@render children()}{:else}{wort ??
			bedeutungsWort(bedeutung)}{/if}</span
>

<style>
	.marke {
		display: inline-flex;
		align-items: center;
		gap: 0.4em;
		font-size: var(--text-xs);
		line-height: 1.4;
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--ton-soft, var(--plan-soft));
		color: var(--text);
		white-space: nowrap;
	}
	.punkt {
		width: 0.55em;
		height: 0.55em;
		border-radius: 50%;
		background: var(--ton, var(--plan));
		flex: none;
	}
</style>
