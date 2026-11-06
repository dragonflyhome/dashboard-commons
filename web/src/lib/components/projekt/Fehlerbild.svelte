<script lang="ts">
	/**
	 * Das Fehlerbild einer Projektseite (Etappe 4b): „Diese Seite hat gerade
	 * einen Fehler“ mit Knopf „neu versuchen“. Steht an Stelle der Seite, wenn
	 * sie sich nicht laden ließ, beim Zeichnen einen Fehler warf (Grenze) oder
	 * eine Bedienung schiefging (Rahmen, `onerror` am Fenster). Kopf,
	 * Navigation und der Rest des Dashboards laufen weiter.
	 */
	interface Props {
		/** Was passiert ist, ein Satz. */
		satz: string;
		/** Der Fehler selbst (für den Chat aufklappbar). */
		fehler: unknown;
		/** Pfad der Datei, etwa `projects/x/Seite.svelte` (`null`: unbekannt). */
		datei: string | null;
		/** „neu versuchen“: die Seite noch einmal zeichnen oder laden. */
		neu: () => void;
	}
	let { satz, fehler, datei, neu }: Props = $props();

	function meldung(e: unknown): string {
		if (e instanceof Error) return e.message;
		if (typeof e === 'object' && e !== null) {
			if ('message' in e && typeof e.message === 'string') return e.message;
			try {
				return JSON.stringify(e);
			} catch {
				// zirkulär: dann wie unten
			}
		}
		return String(e);
	}
</script>

<section class="card fehlerbild" role="alert">
	<h2>Diese Seite hat gerade einen Fehler</h2>
	<p>{satz} Der Rest des Dashboards läuft weiter.</p>
	<p>
		<button type="button" class="button primary" onclick={neu}>neu versuchen</button>
	</p>
	<details>
		<summary class="muted small">Meldung für den Chat</summary>
		{#if datei !== null}
			<p class="small"><code>{datei}</code></p>
		{/if}
		<pre class="meldung">{meldung(fehler)}</pre>
	</details>
</section>

<style>
	.fehlerbild {
		margin-top: 16px;
		border-left: 4px solid var(--bad);
	}
	.fehlerbild h2 {
		margin-top: 0;
	}
	.meldung {
		font-family: var(--mono);
		font-size: var(--text-sm);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		padding: 8px 10px;
		border: 1px solid var(--line);
		border-radius: 8px;
		max-height: 240px;
		overflow: auto;
	}
	summary {
		cursor: pointer;
	}
</style>
