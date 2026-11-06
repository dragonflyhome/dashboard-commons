<script lang="ts">
	/**
	 * „Etwas anderes …“: der letzte Knopf in der
	 * Reihe einer Klick-Antwort, ruhiger als die Antworten des Chats. Ein Klick
	 * klappt direkt darunter ein Feld für eigene Worte auf (kein Fenster über
	 * der Seite: am Handy, mit Tastatur und Vorlese-Programm gleich gut), und der
	 * Fokus springt hinein. Das Feld ist mehrzeilig und wächst mit; darunter
	 * „Senden“ und „Abbrechen“. Esc schließt nur, wenn das Feld leer ist; dann
	 * und nach „Abbrechen“ kehrt der Fokus auf den Knopf zurück. „Abbrechen“
	 * klappt nur zu und behält den Text (beim Aufklappen steht er wieder da);
	 * weg ist er nur mit dem kleinen Knopf „Text verwerfen“, den es nur gibt,
	 * wenn etwas im Feld steht (Nachtrag der Abnahme: nichts still verwerfen).
	 *
	 * Der Baustein steht in einer Reihe mit `flex-wrap` (wie `.knoepfe`): Das
	 * Feld nimmt dort die ganze Breite und rückt so unter die Knöpfe.
	 *
	 * Der Entwurf steht je Tab in sessionStorage (`entwurf`): Er geht beim
	 * Neuladen, bei Live-Änderungen und beim Seitenwechsel nicht verloren, und
	 * mit Text im Feld ist es beim nächsten Mal wieder offen. Darum hält ein
	 * gesicherter Entwurf (`data-entwurf-gesichert`: der Text steht so in
	 * sessionStorage) das Neuladen einer neuen Fassung nicht auf; nur solange
	 * der Fokus im Feld liegt, wartet es (`eingabeOffen`).
	 *
	 * `senden` schickt den Text denselben Weg wie ein Klick und gibt einen Satz
	 * zurück, wenn es nicht ging (er steht dann im Feld), sonst `null`.
	 */
	import { tick, untrack } from 'svelte';
	import {
		EIGENE_HOECHSTENS,
		KNOPF_TEXT,
		SATZ_HINWEIS,
		SATZ_LEER,
		entwurfLesen,
		entwurfSchreiben
	} from '#lib/etwas-anderes.ts';

	interface Props {
		/** Schlüssel des Entwurfs (`frageEntwurf`, `aufgabeEntwurf`). */
		entwurf: string;
		/** Worauf die Antwort sich bezieht (Titel der Frage oder Aufgabe), für Vorlese-Programme. */
		bezug: string;
		/** Gerade läuft eine andere Antwort: Senden ist gesperrt. */
		gesperrt?: boolean;
		senden: (eigene: string) => Promise<string | null>;
		/** Große Knöpfe wie im Kasten „Für dich“. */
		gross?: boolean;
		/** Meldet, ob das Feld offen ist (etwa damit die Reihe die ganze Breite nimmt). */
		onumschalten?: ((offen: boolean) => void) | undefined;
	}
	let { entwurf, bezug, gesperrt = false, senden, gross = false, onumschalten }: Props = $props();

	const kennung = $props.id();
	let text = $state(untrack(() => entwurfLesen(entwurf)));
	let offen = $state(untrack(() => text !== ''));
	let fehler = $state<string | null>(null);
	let laeuft = $state(false);
	let knopf = $state<HTMLButtonElement | null>(null);
	let feld = $state<HTMLTextAreaElement | null>(null);

	$effect(() => {
		onumschalten?.(offen);
	});

	/** Das Feld wächst mit dem Text (ohne eigene Bildlaufleiste). */
	function wachsen(): void {
		if (feld === null) return;
		feld.style.height = 'auto';
		feld.style.height = `${String(feld.scrollHeight)}px`;
	}

	async function oeffnen(): Promise<void> {
		offen = true;
		fehler = null;
		await tick();
		wachsen();
		feld?.focus();
	}

	async function schliessen(): Promise<void> {
		offen = false;
		fehler = null;
		await tick();
		knopf?.focus();
	}

	/** Liegt der Fokus noch hier (oder nirgends)? Dann darf der Baustein ihn setzen. */
	function fokusHier(): boolean {
		const aktiv = document.activeElement;
		return aktiv === null || aktiv === document.body || (feld?.form?.contains(aktiv) ?? false);
	}

	function eingabe(): void {
		entwurfSchreiben(entwurf, text);
		if (fehler === SATZ_LEER && text.trim() !== '') fehler = null;
		wachsen();
	}

	function taste(ev: KeyboardEvent): void {
		if (ev.key !== 'Escape' || text.trim() !== '') return;
		ev.preventDefault();
		ev.stopPropagation();
		text = '';
		entwurfSchreiben(entwurf, '');
		void schliessen();
	}

	/** Zuklappen; der Text bleibt (im Feld und als Entwurf). */
	function abbrechen(): void {
		entwurfSchreiben(entwurf, text);
		void schliessen();
	}

	/** Nur hier wird ein Text verworfen; das Feld bleibt offen, der Fokus darin. */
	async function verwerfen(): Promise<void> {
		text = '';
		entwurfSchreiben(entwurf, '');
		fehler = null;
		await tick();
		wachsen();
		feld?.focus();
	}

	async function absenden(ev: SubmitEvent): Promise<void> {
		ev.preventDefault();
		if (laeuft || gesperrt) return;
		const eigene = text.trim();
		if (eigene === '') {
			fehler = SATZ_LEER;
			feld?.focus();
			return;
		}
		laeuft = true;
		fehler = null;
		try {
			const satz = await senden(eigene);
			if (satz !== null) {
				fehler = satz;
				await tick();
				if (feld?.isConnected) feld.focus();
				return;
			}
			// Nur leeren, was noch genau so dasteht (wie bei der Anmerkung).
			if (text.trim() === eigene) {
				text = '';
				entwurfSchreiben(entwurf, '');
			}
			const zurueck = fokusHier();
			offen = false;
			await tick();
			if (zurueck && knopf?.isConnected) knopf.focus();
		} finally {
			laeuft = false;
		}
	}
</script>

<button
	bind:this={knopf}
	type="button"
	class="button etwas-anderes"
	class:gross
	aria-expanded={offen}
	aria-controls={offen ? `${kennung}-feld` : undefined}
	aria-label="{KNOPF_TEXT}: {bezug}"
	onclick={() => {
		if (offen) feld?.focus();
		else void oeffnen();
	}}>{KNOPF_TEXT}</button
>
{#if offen}
	<form
		id="{kennung}-feld"
		class="ea-feld"
		aria-label="Etwas anderes: {bezug}"
		aria-busy={laeuft}
		onsubmit={absenden}
	>
		<label for="{kennung}-text">In eigenen Worten</label>
		<textarea
			id="{kennung}-text"
			bind:this={feld}
			bind:value={text}
			rows="3"
			maxlength={EIGENE_HOECHSTENS}
			readonly={laeuft}
			data-entwurf-gesichert={entwurfLesen(entwurf) === text ? '' : undefined}
			aria-invalid={fehler !== null}
			aria-describedby={fehler === null
				? `${kennung}-hinweis`
				: `${kennung}-fehler ${kennung}-hinweis`}
			oninput={eingabe}
			onkeydown={taste}></textarea>
		{#if fehler !== null}
			<p id="{kennung}-fehler" class="block-error" role="alert">{fehler}</p>
		{/if}
		<p id="{kennung}-hinweis" class="muted small">
			{SATZ_HINWEIS} Esc schließt das Feld, solange es leer ist.
		</p>
		<div class="ea-knoepfe">
			<button type="submit" class="button primary" class:gross disabled={gesperrt || laeuft}
				>Senden</button
			>
			<button type="button" class="button" class:gross disabled={laeuft} onclick={abbrechen}
				>Abbrechen</button
			>
			{#if text !== ''}
				<button type="button" class="button verwerfen" disabled={laeuft} onclick={verwerfen}
					>Text verwerfen</button
				>
			{/if}
		</div>
		{#if laeuft}
			<p class="muted small">Wird gesendet …</p>
		{/if}
	</form>
{/if}

<style>
	.etwas-anderes {
		background: transparent;
		border-style: dashed;
		color: var(--muted);
		min-height: 2.75rem;
	}
	.etwas-anderes:hover,
	.etwas-anderes[aria-expanded='true'] {
		color: var(--text);
	}
	.etwas-anderes[aria-expanded='true'] {
		border-style: solid;
		border-color: var(--accent);
	}
	.gross {
		min-height: 44px;
		padding: 9px 18px;
		font-weight: 600;
	}
	.ea-feld {
		flex: 1 1 100%;
		display: grid;
		gap: 0.3rem;
		min-width: 0;
		margin: 0;
	}
	label {
		font-size: var(--text-sm);
		color: var(--muted);
	}
	textarea {
		width: 100%;
		box-sizing: border-box;
		min-width: 0;
		min-height: 4.5rem;
		font: inherit;
		color: var(--text);
		background: var(--card);
		border: 1px solid var(--line);
		border-radius: 0.5rem;
		padding: 0.4rem 0.55rem;
		resize: vertical;
		overflow-wrap: anywhere;
	}
	textarea:focus-visible {
		outline: 0.125rem solid var(--accent);
		outline-offset: 0.0625rem;
	}
	textarea[aria-invalid='true'] {
		border-color: var(--bad);
	}
	.ea-knoepfe {
		display: flex;
		flex-wrap: wrap;
		gap: 0.35rem;
	}
	.ea-knoepfe .verwerfen {
		margin-left: auto;
		background: transparent;
		border-color: transparent;
		color: var(--muted);
		font-size: var(--text-sm);
		text-decoration: underline;
	}
	.ea-knoepfe .button {
		min-height: 2.75rem;
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
