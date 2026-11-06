<script lang="ts">
	/**
	 * Dialog (Etappe 3a-3, Bits UI Dialog): ein Fenster über der Seite für
	 * eine Rückfrage oder eine kurze Eingabe. Öffnet mit dem Knopf, hält den
	 * Fokus in sich (Tab läuft im Kreis), Escape und „Schließen“ schließen,
	 * danach steht der Fokus wieder auf dem Knopf. Die Seite dahinter rollt
	 * nicht mit.
	 *
	 *   <Dialog titel="Wirklich löschen?" ausloeser="Löschen">
	 *     {#snippet children(schliessen)}<Knopf onclick={schliessen}>Ja</Knopf>{/snippet}
	 *   </Dialog>
	 */
	import type { Snippet } from 'svelte';
	import { Dialog } from 'bits-ui';

	interface Props {
		titel: string;
		/** Ein Satz unter dem Titel (aria-describedby). */
		beschreibung?: string | undefined;
		/** Beschriftung des Knopfs, der den Dialog öffnet. */
		ausloeser: string;
		offen?: boolean;
		/** Inhalt; bekommt `schliessen` für eigene Knöpfe. */
		children: Snippet<[() => void]>;
	}
	let { titel, beschreibung, ausloeser, offen = $bindable(false), children }: Props = $props();

	function schliessen(): void {
		offen = false;
	}
</script>

<Dialog.Root bind:open={offen}>
	<Dialog.Trigger>
		{#snippet child({ props })}
			<button {...props} class="button">{ausloeser}</button>
		{/snippet}
	</Dialog.Trigger>
	<Dialog.Portal>
		<Dialog.Overlay>
			{#snippet child({ props })}
				<div {...props} class="schleier"></div>
			{/snippet}
		</Dialog.Overlay>
		<Dialog.Content>
			{#snippet child({ props })}
				<div {...props} class="fenster">
					<Dialog.Title>
						{#snippet child({ props: t })}
							<h2 {...t}>{titel}</h2>
						{/snippet}
					</Dialog.Title>
					{#if beschreibung}
						<Dialog.Description>
							{#snippet child({ props: b })}
								<p {...b} class="muted">{beschreibung}</p>
							{/snippet}
						</Dialog.Description>
					{/if}
					<div class="inhalt">{@render children(schliessen)}</div>
					<div class="fuss">
						<Dialog.Close>
							{#snippet child({ props: z })}
								<button {...z} class="button">Schließen</button>
							{/snippet}
						</Dialog.Close>
					</div>
				</div>
			{/snippet}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<style>
	.schleier {
		position: fixed;
		inset: 0;
		z-index: 60;
		background: var(--overlay);
	}
	.fenster {
		position: fixed;
		top: 50%;
		left: 50%;
		z-index: 61;
		width: min(32rem, calc(100vw - 2rem));
		max-height: calc(100vh - 2rem);
		overflow-y: auto;
		transform: translate(-50%, -50%);
		padding: 1rem 1.1rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--card);
		color: var(--text);
		box-shadow: var(--shadow-gross);
		overflow-wrap: anywhere;
	}
	h2 {
		margin-top: 0;
	}
	.inhalt {
		margin: 0.6rem 0;
	}
	.fuss {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 0.5rem;
	}
</style>
