<script lang="ts">
	/**
	 * Karte „Sicherung“: Ampel, letzte Sicherung, Ordner und der Knopf
	 * „Jetzt sichern“. `sichern` und `nachher` kommen von der Seite, damit der
	 * Test sie ersetzen kann.
	 */
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { fehlerText } from '#lib/api/client.ts';
	import { fmtDate } from '#lib/format.ts';
	import type { DatenBericht, SicherungAntwort } from '$vertrag';

	interface Props {
		bericht: DatenBericht;
		/** Legt eine Sicherung an (POST /api/daten/sichern). */
		sichern: () => Promise<SicherungAntwort>;
		/** Lädt die Seite danach neu, z. B. `invalidate('api:daten')`. */
		nachher?: () => Promise<void> | void;
	}
	let { bericht, sichern, nachher }: Props = $props();

	const ampel = {
		ok: { bedeutung: 'ok', wort: 'Sicherung ist aktuell' },
		warn: { bedeutung: 'wartet', wort: 'Sicherung ist etwas älter' },
		bad: { bedeutung: 'problem', wort: 'Sicherung fehlt oder ist zu alt' }
	} as const;
	const stand = $derived(ampel[bericht.sicherung_klasse]);
	/** Ohne Datenspeicher kann der Server nichts sichern (503). */
	const ohneSpeicher = $derived(Boolean(bericht.fehler));

	let laeuft = $state(false);
	let erfolg = $state<string | null>(null);
	let fehler = $state<string | null>(null);

	async function jetztSichern(): Promise<void> {
		if (ohneSpeicher) return;
		laeuft = true;
		erfolg = null;
		fehler = null;
		try {
			const antwort = await sichern();
			erfolg = `Sicherung vom ${fmtDate(antwort.datum.slice(0, 10))} angelegt.`;
			await nachher?.();
		} catch (e) {
			fehler = fehlerText(e);
		} finally {
			laeuft = false;
		}
	}
</script>

<p class="big stand">
	<StatusDot bedeutung={stand.bedeutung} label={stand.wort} />
	{#if bericht.letzte_sicherung}
		Letzte Sicherung: {fmtDate(bericht.letzte_sicherung)}
	{:else}
		Letzte Sicherung: noch keine
	{/if}
</p>
<p>Jede Nacht legt der Server eine Kopie an und behält die letzten 14.</p>
<p class="muted small">Ordner: <code class="pfad">{bericht.sicherungs_ordner}</code></p>
<p>
	<button
		type="button"
		class="button primary"
		disabled={laeuft || ohneSpeicher}
		aria-describedby={ohneSpeicher ? 'sichern-gesperrt' : undefined}
		onclick={jetztSichern}
	>
		{laeuft ? 'Sichere …' : 'Jetzt sichern'}
	</button>
</p>
{#if ohneSpeicher}
	<p class="muted small" id="sichern-gesperrt">
		Sichern geht erst wieder, wenn der Datenspeicher offen ist.
	</p>
{/if}
<div aria-live="polite">
	{#if erfolg}<p class="ok">{erfolg}</p>{/if}
	{#if fehler}<p class="block-error">{fehler}</p>{/if}
</div>

<style>
	.stand {
		display: flex;
		align-items: center;
	}
	.pfad {
		overflow-wrap: anywhere;
	}
	.ok {
		color: var(--ok-text);
		font-weight: 600;
	}
	button[disabled] {
		opacity: 0.7;
		cursor: wait;
	}
	button[disabled][aria-describedby] {
		cursor: not-allowed;
	}
</style>
