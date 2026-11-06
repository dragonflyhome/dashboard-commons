<script lang="ts">
	/**
	 * Das Schloss rechts neben dem Live-Punkt (Kopfleiste, Fassung 2,
	 * `[fassung_2_schloss]`): offen = angemeldet, zu = nicht angemeldet. Der
	 * Zustand kommt vom Server (freier Kanal und geschützte Anfrage, siehe
	 * `verbindung.ts`). Ein Klick führt immer zur Passkey-Seite aus 7c:
	 * angemeldet zum Anlegen eines Passkeys und zur Liste, nicht registriert
	 * zum Antrag, sonst zum Anmelden; danach geht es zur Seite von hier zurück.
	 * Ziel 44 × 44 px, Farbe nur über Token.
	 *
	 * Kein Baustein im `$katalog`: Der Katalog ist für Projektseiten, das
	 * Schloss hängt an der Anmeldung der App selbst (wie Live-Punkt und Menü).
	 *
	 * `zustand` nur für Tests; sonst der freie Kanal.
	 */
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { freierKanal } from '#lib/freikanal.svelte.ts';
	import { schlossName, schlossOffen, schlossZiel, type LiveZustand } from '#lib/verbindung.ts';

	interface Props {
		zustand?: LiveZustand | null;
	}
	let { zustand: vorgegeben }: Props = $props();

	const zustand = $derived(vorgegeben === undefined ? freierKanal.zustand : vorgegeben);
	// Solange der erste Versuch läuft: zu, wie ohne Anmeldung (das Ziel ist dann Anmelden).
	const z = $derived(zustand ?? 'nicht_angemeldet');
	const offen = $derived(schlossOffen(z));
	const name = $derived(schlossName(z));
	const ziel = $derived(schlossZiel(z, `${page.url.pathname}${page.url.search}`));
	// `schlossZiel` gibt „/anmelden?…#…“; `resolve` nimmt nur den Pfad.
	const href = $derived(`${resolve('/anmelden')}${ziel.slice('/anmelden'.length)}`);
</script>

<!-- eslint-disable svelte/no-navigation-without-resolve -- baut mit resolve(), dazu Rückweg und Sprungmarke -->
<a
	class="schloss"
	class:offen
	{href}
	aria-label={name}
	title={name}
	data-schloss={offen ? 'offen' : 'zu'}
>
	<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" focusable="false">
		{#if offen}
			<!-- Bügel offen: links hochgezogen -->
			<path d="M7 11V7a5 5 0 0 1 9.6-1.9" />
		{:else}
			<path d="M7 11V7a5 5 0 0 1 10 0v4" />
		{/if}
		<rect x="4.5" y="11" width="15" height="10" rx="2" />
		<circle cx="12" cy="16" r="1.4" class="loch" />
	</svg>
</a>

<!-- eslint-enable svelte/no-navigation-without-resolve -->

<style>
	.schloss {
		display: inline-flex;
		flex: none;
		align-items: center;
		justify-content: center;
		width: 2.75rem;
		height: 2.75rem;
		border: 1px solid var(--line);
		border-radius: 0.625rem;
		color: var(--muted);
		background: var(--card);
	}
	.schloss:hover {
		color: var(--text);
		border-color: var(--muted);
	}
	.schloss.offen {
		color: var(--ok-text);
		border-color: var(--ok);
		background: var(--ok-soft);
	}
	svg {
		width: 1.375rem;
		height: 1.375rem;
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.loch {
		fill: currentColor;
		stroke: none;
	}
</style>
