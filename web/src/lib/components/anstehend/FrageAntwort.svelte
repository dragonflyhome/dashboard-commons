<script lang="ts">
	/**
	 * Eine Frage direkt beantworten (Etappe A2): Knöpfe bei „wahl“ (je Option
	 * einer, die Empfehlung des Chats ist vermerkt) und „ja-nein“, ein Textfeld
	 * mit „Absenden“ bei „text“, ein Knopf bei „bestätigen“. Bei allen außer
	 * „text“ gibt es ein aufklappbares Feld „Anmerkung (freiwillig)“: Wer es
	 * nicht öffnet, antwortet weiter mit einem Klick; was darin steht, geht mit
	 * dem nächsten Klick als `notiz` mit. Ein Klick (oder
	 * Absenden) schickt die Antwort; die Rückmeldung zeigt `AntwortStatus`
	 * dort, wo die Frage stand, weil die Frage nach dem Neuladen verschwindet.
	 *
	 * Bei „wahl“, „ja-nein“ und „bestätigen“ steht als
	 * letzter Knopf „Etwas anderes …“ (`EtwasAnderes`); er klappt darunter ein
	 * Feld für eigene Worte auf. Gesendet wird derselbe Weg wie ein Klick, mit
	 * `antwort = "etwas-anderes"` und dem Text in `eigene`; eine offene
	 * Anmerkung geht mit. Bei „text“ gibt es ihn nicht (dort sind es ohnehin
	 * eigene Worte).
	 *
	 * Steht bei einem Eintrag keine Angabe zur Frage (`frage` ist null), zeigt
	 * der Baustein nichts.
	 */
	import { NOTIZ_HOECHSTENS, SATZ_NETZ, TEXT_HOECHSTENS, artWort } from '#lib/antworten.ts';
	import { ETWAS_ANDERES, frageEntwurf } from '#lib/etwas-anderes.ts';
	import EtwasAnderes from '../EtwasAnderes.svelte';
	import { antworten, type Antworten } from '#lib/antworten.svelte.ts';
	import type { AnstehendEintrag } from '$vertrag';

	interface Props {
		eintrag: AnstehendEintrag;
		/** Nach dem Senden (auch nach einem Fehler): die Seite lädt neu. */
		onbeantwortet?: (() => void | Promise<void>) | undefined;
		/** Wo die Frage steht (`anstehend`, `start`, `projekt:<slug>`): nur dort nimmt die Rückmeldung den Fokus. */
		abschnitt?: string;
		/** Für Tests; sonst der gemeinsame Speicher. */
		speicher?: Antworten;
	}
	let { eintrag: e, onbeantwortet, abschnitt = '', speicher = antworten }: Props = $props();

	const kennung = $props.id();
	let text = $state('');
	let notiz = $state('');
	const angabe = $derived(e.frage);
	const laeuft = $derived(speicher.beschaeftigt === `${e.projekt}/${e.id}`);
	const gesperrt = $derived(speicher.beschaeftigt !== null);
	const textLeer = $derived(text.trim().length === 0);

	/**
	 * Nach einer gespeicherten Antwort sind Anmerkung und Text wieder leer: Sie
	 * gehörten zu dieser Antwort und gehen nie still mit der nächsten mit.
	 * Geleert wird nur, was noch genau so dasteht. Dazu schlüsseln die Aufrufer
	 * den Baustein nach `frageSchluessel`, damit die nächste Frage einen neuen
	 * bekommt.
	 */
	function senden(antwort: string | null): void {
		const mitNotiz = angabe !== null && angabe.art !== 'text' ? notiz : null;
		const mitText = text;
		void speicher.senden(e, antwort, mitNotiz, onbeantwortet, abschnitt).then((gut) => {
			if (!gut) return;
			if (notiz === mitNotiz) notiz = '';
			if (text === mitText) text = '';
		});
	}

	/**
	 * „Etwas anderes …“: dieselbe Antwort wie ein Klick, mit eigenen Worten.
	 * Gibt den Satz zurück, wenn es nicht ging (er steht dann im Feld).
	 */
	async function eigeneSenden(eigene: string): Promise<string | null> {
		const mitNotiz = notiz;
		const gut = await speicher.senden(e, ETWAS_ANDERES, mitNotiz, onbeantwortet, abschnitt, eigene);
		if (!gut) return speicher.meldung?.satz ?? SATZ_NETZ;
		if (notiz === mitNotiz) notiz = '';
		return null;
	}
</script>

{#snippet etwasAnderes()}
	<EtwasAnderes entwurf={frageEntwurf(e)} bezug={e.titel} {gesperrt} senden={eigeneSenden} />
{/snippet}

{#if angabe}
	<div
		class="antwort"
		role="group"
		aria-label="Antwort auf „{e.titel}“ ({artWort(angabe.art)})"
		aria-busy={laeuft}
	>
		{#if angabe.art === 'wahl'}
			<div class="knoepfe">
				{#each angabe.optionen as o (o)}
					<button
						type="button"
						class="button"
						class:empfohlen={o === angabe.empfehlung}
						disabled={gesperrt}
						onclick={() => {
							senden(o);
						}}
						>{o}{#if o === angabe.empfehlung}<span class="vermerk">&#32;(empfohlen)</span
							>{/if}</button
					>
				{/each}
				{@render etwasAnderes()}
			</div>
		{:else if angabe.art === 'ja-nein'}
			<div class="knoepfe">
				<button
					type="button"
					class="button"
					disabled={gesperrt}
					onclick={() => {
						senden('ja');
					}}>Ja</button
				>
				<button
					type="button"
					class="button"
					disabled={gesperrt}
					onclick={() => {
						senden('nein');
					}}>Nein</button
				>
				{@render etwasAnderes()}
			</div>
		{:else if angabe.art === 'text'}
			<form
				class="text"
				onsubmit={(ev) => {
					ev.preventDefault();
					if (!textLeer) senden(text);
				}}
			>
				<label for="{kennung}-text">Deine Antwort</label>
				<textarea
					id="{kennung}-text"
					rows="3"
					maxlength={TEXT_HOECHSTENS}
					bind:value={text}
					disabled={gesperrt}></textarea>
				<div class="knoepfe">
					<button type="submit" class="button" disabled={gesperrt || textLeer}>Absenden</button>
				</div>
			</form>
		{:else}
			<div class="knoepfe">
				<button
					type="button"
					class="button"
					disabled={gesperrt}
					onclick={() => {
						senden(null);
					}}><span aria-hidden="true">✓</span> Bestätigen</button
				>
				{@render etwasAnderes()}
			</div>
		{/if}
		{#if angabe.art !== 'text'}
			<details class="anmerkung">
				<summary>Anmerkung (freiwillig)</summary>
				<div class="anmerkung-feld">
					<label for="{kennung}-notiz">Deine Anmerkung</label>
					<textarea
						id="{kennung}-notiz"
						aria-describedby="{kennung}-notiz-hinweis"
						rows="2"
						maxlength={NOTIZ_HOECHSTENS}
						bind:value={notiz}
						disabled={gesperrt}></textarea>
					<p id="{kennung}-notiz-hinweis" class="muted small">
						Erst schreiben, dann oben auf deine Antwort klicken. Die Anmerkung geht mit.
					</p>
				</div>
			</details>
		{/if}
		{#if laeuft}
			<p class="muted small">Wird gesendet …</p>
		{/if}
	</div>
{/if}

<style>
	.antwort {
		display: grid;
		gap: 0.4rem;
		margin-top: 0.3rem;
		min-width: 0;
	}
	.knoepfe {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}
	.knoepfe .button {
		min-height: 2.75rem;
		max-width: 100%;
		overflow-wrap: anywhere;
		white-space: normal;
		text-align: left;
	}
	.empfohlen {
		border-color: var(--accent);
	}
	.vermerk {
		color: var(--muted);
		font-size: var(--text-em-sm);
	}
	.anmerkung {
		min-width: 0;
	}
	.anmerkung-feld {
		display: grid;
		gap: 0.3rem;
		min-width: 0;
		margin-top: 0.3rem;
	}
	.anmerkung summary {
		cursor: pointer;
		font-size: var(--text-sm);
		color: var(--muted);
		width: fit-content;
	}
	.text {
		display: grid;
		gap: 0.3rem;
		min-width: 0;
	}
	label {
		font-size: var(--text-sm);
		color: var(--muted);
	}
	textarea {
		width: 100%;
		box-sizing: border-box;
		min-width: 0;
		font: inherit;
		color: var(--text);
		background: var(--card);
		border: 1px solid var(--line);
		border-radius: 0.5rem;
		padding: 0.4rem 0.55rem;
		resize: vertical;
	}
	textarea:focus-visible {
		outline: 0.125rem solid var(--accent);
		outline-offset: 0.0625rem;
	}
	p {
		margin: 0;
	}
	@media (prefers-reduced-motion: reduce) {
		.button {
			transition: none;
		}
	}
</style>
