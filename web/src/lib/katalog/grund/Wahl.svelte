<script lang="ts">
	/**
	 * Wahl (Etappe 3b, Bits UI RadioGroup): eine aus wenigen Möglichkeiten,
	 * nebeneinander wie Segmente, sofort wirksam. Tab geht auf die gewählte,
	 * die Pfeile wechseln (und wählen gleich); Vorlese-Programme hören
	 * „Optionsfeld, ausgewählt, 1 von 3“ mit dem Namen der Gruppe. Jede
	 * Möglichkeit ist mindestens 44 px hoch (Ziel für Finger).
	 *
	 *   <Wahl bind:wert={ansicht} beschriftung="Ansicht"
	 *     optionen={[{ wert: 'kacheln', text: 'Kacheln' }, { wert: 'spalten', text: 'Spalten' }]} />
	 *
	 * Etappe S2, Karten-Form: Mit dem Snippet `karte` zeichnet jede Möglichkeit
	 * ihren eigenen Inhalt, und die Möglichkeiten stehen als Raster von Karten
	 * (eine Spalte schmal, zwei ab 640 px, mehr ab 1024 px). Tastatur und Rollen
	 * sind dieselben, alle vier Pfeile wechseln. Der Name eines Optionsfelds ist
	 * dann nur das Element mit `ids.name`, die Beschreibung das mit
	 * `ids.beschreibung`; im Knopf stehen nur `span`-Elemente und nichts
	 * Bedienbares. Die gewählte Karte trägt einen kräftigen Rand und einen Haken.
	 *
	 *   <Wahl bind:wert={thema} beschriftung="Farben" optionen={THEMEN}>
	 *     {#snippet karte(o, ids)}
	 *       <span id={ids.name}>{o.text}</span>
	 *       <span id={ids.beschreibung}>…</span>
	 *     {/snippet}
	 *   </Wahl>
	 */
	import type { Snippet } from 'svelte';
	import { RadioGroup } from 'bits-ui';
	import type { KartenIds, Option } from './typen.ts';

	interface Props {
		wert?: string;
		/** Name der Gruppe; steht davor, mit `beschriftungZeigen={false}` nur für Vorlese-Programme. */
		beschriftung: string;
		beschriftungZeigen?: boolean;
		optionen: readonly Option[];
		onwechsel?: ((wert: string) => void) | undefined;
		/** Karten-Form: der Inhalt je Möglichkeit; ohne Snippet Segmente wie bisher. */
		karte?: Snippet<[Option, KartenIds]>;
	}
	let {
		wert = $bindable(''),
		beschriftung,
		beschriftungZeigen = true,
		optionen,
		onwechsel,
		karte
	}: Props = $props();
	const id = $props.id();
</script>

<div class="wahl" class:mit-karten={karte !== undefined}>
	<span id="{id}-name" class="name" class:nur-vorlesen={!beschriftungZeigen}>{beschriftung}</span>
	<RadioGroup.Root
		bind:value={wert}
		orientation="horizontal"
		aria-labelledby="{id}-name"
		onValueChange={(v) => onwechsel?.(v)}
	>
		{#snippet child({ props })}
			{#if karte === undefined}
				<div {...props} class="segmente">
					{#each optionen as o (o.wert)}
						<RadioGroup.Item value={o.wert} disabled={o.gesperrt ?? false}>
							{#snippet child({ props: knopf })}
								<button {...knopf} class="segment">{o.text}</button>
							{/snippet}
						</RadioGroup.Item>
					{/each}
				</div>
			{:else}
				<div {...props} class="karten">
					{#each optionen as o, i (o.wert)}
						{@const ids = {
							name: `${id}-k${String(i)}-name`,
							beschreibung: `${id}-k${String(i)}-text`
						}}
						<RadioGroup.Item value={o.wert} disabled={o.gesperrt ?? false}>
							{#snippet child({ props: knopf })}
								<button
									{...knopf}
									class="karte"
									aria-labelledby={ids.name}
									aria-describedby={ids.beschreibung}
								>
									<span class="haken" aria-hidden="true">✓</span>
									{@render karte(o, ids)}
								</button>
							{/snippet}
						</RadioGroup.Item>
					{/each}
				</div>
			{/if}
		{/snippet}
	</RadioGroup.Root>
</div>

<style>
	.wahl {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.25rem 0.6rem;
	}
	.name {
		color: var(--muted);
		font-size: var(--text-sm);
	}
	.segmente {
		display: inline-flex;
		padding: 0.125rem;
		gap: 0.125rem;
		border: 1px solid var(--line);
		border-radius: 999px;
		background: var(--card);
	}
	.segment {
		min-height: 2.75rem;
		min-width: 2.75rem;
		padding: 0 0.9rem;
		border: 0;
		border-radius: 999px;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--text-sm);
		cursor: pointer;
	}
	.segment:hover {
		background: var(--accent-soft);
	}
	.segment[data-state='checked'] {
		background: var(--accent);
		color: var(--on-accent);
		font-weight: 600;
	}
	/* Karten-Form (Etappe S2): die Gruppe nimmt die Breite, der Name steht darüber. */
	.wahl.mit-karten {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 0.5rem;
	}
	.karten {
		display: grid;
		/* Mit fester Breite misst das Raster (auto-fit, Kind einer Flex-Spalte) seine Höhe nach den echten Spalten, sonst nach gestapelten Karten. */
		width: 100%;
		grid-template-columns: 1fr;
		gap: 0.75rem;
	}
	@media (min-width: 640px) {
		.karten {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
	@media (min-width: 1024px) {
		.karten {
			grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
		}
	}
	.karte {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 0.5rem;
		min-height: 2.75rem;
		padding: 0.75rem;
		/* Rechts Platz für den Haken (right 0.6rem plus Zeichen), damit er nie Text der Karte überdeckt. */
		padding-right: 2rem;
		/* Der Rand ist immer 3 px breit, damit die Wahl nichts verschiebt;
		   ungewählt in der Linienfarbe, gewählt kräftig (--text hat auf --card
		   und --bg mehr als 3:1, kontrast.spec.ts hält das fest). */
		border: 3px solid var(--line);
		border-radius: var(--radius);
		background: var(--card);
		color: var(--text);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}
	.karte:hover {
		background: var(--accent-soft);
	}
	.karte[data-state='checked'] {
		border-color: var(--text);
	}
	/* Der Haken zeigt die Wahl ohne Farbe; den Zustand selbst meldet aria-checked. */
	.haken {
		position: absolute;
		top: 0.4rem;
		right: 0.6rem;
		font-weight: 700;
		visibility: hidden;
	}
	.karte[data-state='checked'] .haken {
		visibility: visible;
	}
	.karte:disabled {
		cursor: not-allowed;
		border-style: dashed;
	}
	/* Gesperrt regelt app.css (gestrichelter Rand, ohne Deckkraft); das Segment hat
	   keinen eigenen Rand, darum hier die gestrichelte Linie innen. */
	.segment:disabled {
		cursor: not-allowed;
		outline: 1px dashed var(--muted);
		outline-offset: -0.2rem;
	}
</style>
