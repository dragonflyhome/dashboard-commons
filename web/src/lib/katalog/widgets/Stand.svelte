<script lang="ts">
	/**
	 * Widget „Stand“ (Etappe 3a-3, A4), gewählter Entwurf „Kachel mit Ampel“:
	 * oben ein Rand in der Farbe der Ampel, darunter Punkt und Satz des
	 * Servers, dann die Zahlen in einer Zeile (wartet auf dich, läuft, als
	 * Nächstes; nur, was nicht null ist), dann Status, Schwerpunkt und
	 * „zuletzt geändert“. Die Ampel ist eine Regel des Servers; Satz und
	 * Zahlen kommen aus `standAnsicht` (anstehend.ts), Farbe und Wörter
	 * wählt dieses Widget.
	 *
	 * Warum diese Form: Die Ampel allein sagt nicht, wie viel ansteht; die
	 * kompakte Karte (bisher) brauchte dafür die anderen Karten. Die Zahlen
	 * stehen hier ruhig als Wörter mit Zahl, die Farbe nie allein (Punkt mit
	 * Namen „Ampel gelb“, Satz daneben).
	 */
	import type { GesamtAnsicht, KurzAnsicht } from '#lib/anstehend.ts';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { klasse } from '#lib/bedeutung.ts';
	import { AMPEL } from './worte.ts';

	interface Props {
		gesamt: GesamtAnsicht;
		schwerpunkt: string;
		kurz?: KurzAnsicht | null;
		/** Der Datenspeicher fehlt gerade (ein Satz dazu). */
		speicherFehlt?: boolean;
	}
	let { gesamt, schwerpunkt, kurz = null, speicherFehlt = false }: Props = $props();

	// Archiviert (`ruhig`): keine Ampel und keine Farbe, nur der ruhige Satz.
	const ruhig = $derived(gesamt.ruhig === true);
	const a = $derived(ruhig ? { bedeutung: 'neutral' as const, wort: '' } : AMPEL[gesamt.ampel]);
	const zahlen = $derived(
		kurz === null
			? []
			: [
					{ zahl: kurz.wartet_auf_dich, wort: 'wartet auf dich' },
					{ zahl: kurz.laeuft, wort: 'läuft' },
					{ zahl: kurz.als_naechstes, wort: 'als Nächstes' }
				].filter((z) => z.zahl > 0)
	);
</script>

<div class="karte widget card stand {ruhig ? 'ruhig' : klasse(a.bedeutung)}">
	<h2>Stand</h2>
	<p class="grund">
		{#if !ruhig}<StatusDot bedeutung={a.bedeutung} label="Ampel {a.wort}" />{/if}
		{gesamt.grund}
	</p>
	{#if zahlen.length > 0}
		<ul class="plain zahlen">
			{#each zahlen as z (z.wort)}
				<li><b>{z.zahl}</b> {z.wort}</li>
			{/each}
		</ul>
	{/if}
	<p class="muted small">
		{gesamt.status} · {schwerpunkt}
		{#if gesamt.zuletzt}<br />zuletzt geändert {gesamt.zuletzt}{/if}
	</p>
	{#if speicherFehlt}
		<p class="muted small">
			Aufgaben und Arbeiten fehlen gerade: Der Datenspeicher ist nicht offen. Notizen und Fragen
			stehen trotzdem da.
		</p>
	{/if}
</div>

<style>
	.grund {
		font-weight: 600;
		margin-bottom: 0.3rem;
	}
	.zahlen {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 0.5rem;
		margin-bottom: 0.4rem;
		font-size: var(--text-sm);
	}
	.zahlen li {
		padding: 0.05rem 0.5rem;
		border-radius: 999px;
		background: var(--ton-soft, var(--plan-soft));
	}
</style>
