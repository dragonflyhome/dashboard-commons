<script lang="ts">
	/**
	 * Stand-Hinweis (Etappe B1): „Stand vom 30.10.2026“ an einer
	 * Momentaufnahme (Kennzahl, Ampel-Zeile), damit eine alte Zahl nicht
	 * aktuell aussieht. Wie alt der Stand ist und ab wann er als alt gilt,
	 * rechnet der Server (`stand_alter`: Tage und Stufe, `alt_ab` in der
	 * Datei, ohne Angabe 7 Tage). Alt ist gelb (Bedeutung „wartet“) und sagt
	 * es in Worten („12 Tage alt“); frisch bleibt ruhig.
	 *
	 * Live-Daten des Servers haben ihren eigenen Hinweis (aktualitaet); dieser
	 * hier ist für Werte, die ein Chat von Hand einträgt.
	 *
	 *   <StandHinweis stand="2026-10-30" alter={k.stand_alter} />
	 */
	import type { StandAlter } from '$vertrag';
	import { ausStandStufe, klasse } from '#lib/bedeutung.ts';
	import { fmtDate, istZahl } from '#lib/format.ts';

	interface Props {
		/** JJJJ-MM-TT. */
		stand: string;
		/** Vom Server; ohne Alter nur das Datum. */
		alter?: StandAlter | null | undefined;
	}
	let { stand, alter = null }: Props = $props();

	const alt = $derived(alter?.stufe === 'alt');
	const tage = $derived(alter !== null && istZahl(alter.tage) ? alter.tage : null);
</script>

<span
	class="stand-hinweis {klasse(ausStandStufe(alter?.stufe))}"
	class:alt
	data-stufe={alter?.stufe}
	>{#if alt}<span class="punkt" aria-hidden="true"></span>{/if}Stand vom {fmtDate(
		stand
	)}{#if alt && tage !== null},
		{tage === 1 ? '1 Tag' : `${tage} Tage`} alt{/if}</span
>

<style>
	.stand-hinweis {
		display: inline-flex;
		align-items: center;
		gap: 0.35em;
		font-size: var(--text-xs);
		color: var(--muted);
	}
	.alt {
		color: var(--text);
		background: var(--ton-soft);
		border-radius: 999px;
		padding: 0 8px;
	}
	.punkt {
		width: 0.55em;
		height: 0.55em;
		border-radius: 50%;
		background: var(--ton);
		flex: none;
	}
</style>
