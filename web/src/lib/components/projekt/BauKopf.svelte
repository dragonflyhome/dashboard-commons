<script lang="ts">
	/**
	 * Bau-Stand im Kopf der Projektseite (Etappe 4c): grün, rot, nicht geprüft,
	 * läuft oder noch nicht gebaut; bei grün „in Ordnung“, bei rot „zeigt die
	 * letzte gute Fassung von …“ und, angemeldet, der Satz des Servers. Nicht
	 * geprüft: ohne Anmeldung nur der Zustand, angemeldet darunter der Satz
	 * (wie bei rot, ohne Ansage: ein Hinweis, keine Aufforderung).
	 *
	 * Keine Zeitzeile und kein Knopf zum Bauen
	 * mehr; Daten und App-Fassung sind getrennt. Ob die App im Tab veraltet ist
	 * oder eine neue Fassung gebaut wird, sagt der Live-Punkt (`LiveBadge`).
	 * Ohne eigene Seite (keine Seite.svelte) steht hier nichts. Ausgewertet wird
	 * nur in `#lib/projekte.ts`.
	 *
	 * Bau-Tempo, Zweig C: Läuft der Bau, steht angemeldet der Schritt dahinter
	 * („prüft Projekt 7 von 21“). Nur sichtbar: kein aria-live, kein
	 * role=status oder progressbar (NVDA meldete jede Zahl); angesagt wird nur
	 * das Ergebnis, am Live-Punkt. Ohne Anmeldung kein Zähler.
	 */
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { bauLage, type BauLage } from '#lib/bau.svelte.ts';

	interface Props {
		slug: string;
		/** Nur für Tests: ein eigener Stand statt des gemeinsamen. */
		lage?: BauLage;
	}
	let { slug, lage = bauLage }: Props = $props();

	// Mit dem Stand des Projekts: kein „Seite in Ordnung“ neben einer roten Karte.
	const anzeige = $derived(lage.kopf(slug));
	const schritt = $derived(anzeige?.zustand === 'laeuft' ? lage.schritt : null);
</script>

{#if anzeige !== null}
	<div class="bau" data-bau={anzeige.zustand}>
		<p class="zeile">
			<StatusDot bedeutung={anzeige.bedeutung} decorative /><span>{anzeige.zeile}</span>
			{#if schritt !== null}<span class="schritt muted" data-bau-schritt>{schritt}</span>{/if}
		</p>
		{#if anzeige.satz}
			<p class="satz muted small">{anzeige.satz}</p>
		{/if}
	</div>
{/if}

<style>
	.bau {
		min-width: 0;
		margin: 4px 0 8px;
	}
	.zeile {
		display: flex;
		align-items: baseline;
		margin: 0;
		overflow-wrap: anywhere;
	}
	.satz {
		margin: 2px 0 0;
	}
	.schritt {
		margin-left: 0.5em;
	}
</style>
