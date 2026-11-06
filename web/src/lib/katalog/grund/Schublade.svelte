<script lang="ts">
	/**
	 * Schublade (Kopfleiste, Fassung 3, Bits UI Dialog): ein Knopf (44 px), der
	 * von links eine Leiste über die Seite schiebt, etwa die Seitenleiste am
	 * Handy. Der Fokus bleibt in der Schublade (Tab läuft im Kreis); Escape,
	 * ein Klick daneben und „Schließen“ schließen, danach steht der Fokus
	 * wieder auf dem Knopf.
	 *
	 *   <Schublade beschriftung="Navigation" titel="Seiten" bind:offen>
	 *     <Seitenleiste />
	 *   </Schublade>
	 *
	 * Ohne Seitensperre (Lehre aus dem Menü-Hotfix): `preventScroll`
	 * setzt sonst `pointer-events: none` und `overflow: hidden` auf `<body>`;
	 * bleibt das nach einem Seitenwechsel stehen, ist die ganze Seite tot. Der
	 * Schleier fängt Klicks daneben ab, das reicht. Wer die Schublade bei einem
	 * Seitenwechsel schließen will, setzt `offen` auf `false` (die Kopfleiste
	 * tut das in `afterNavigate`).
	 *
	 * Fokus nach dem Schließen (Auflage aus der Abnahme): Ist der Knopf nicht
	 * mehr zu sehen (etwa weil das Fenster breiter als 1024 px wurde und die
	 * Kopfleiste ihn ausblendet), springt der Fokus nicht auf den versteckten
	 * Knopf, sondern auf `ersatz` (sonst `<main>`). Ein Doppelklick auf den
	 * Knopf öffnet und lässt offen: Ein Klick daneben in den ersten 400 ms
	 * nach dem Öffnen zählt nicht als „daneben“.
	 */
	import type { Snippet } from 'svelte';
	import { Dialog } from 'bits-ui';

	interface Props {
		/** Name des Knopfs für Vorlese-Programme (er zeigt nur das Zeichen ☰). */
		beschriftung: string;
		/** Überschrift in der Schublade (für Vorlese-Programme, sichtbar klein). */
		titel: string;
		offen?: boolean;
		/** Fokusziel nach dem Schließen, wenn der Knopf nicht zu sehen ist (sonst `<main>`). */
		ersatz?: () => HTMLElement | null;
		children: Snippet;
	}
	let { beschriftung, titel, offen = $bindable(false), ersatz, children }: Props = $props();

	/** So lange nach dem Öffnen zählt ein Klick daneben nicht (zweiter Klick eines Doppelklicks). */
	const SCHONFRIST_MS = 400;

	let knopf = $state<HTMLButtonElement | null>(null);
	let geoeffnetUm = 0;
	$effect(() => {
		if (offen) geoeffnetUm = performance.now();
	});

	/** Der zweite Klick eines Doppelklicks landet auf dem Schleier: nicht schließen. */
	function daneben(e: Event): void {
		if (performance.now() - geoeffnetUm < SCHONFRIST_MS) e.preventDefault();
	}

	/** Nach dem Schließen: zurück auf den Knopf, nur wenn man ihn sieht. */
	function fokusDanach(e: Event): void {
		if (knopf !== null && knopf.isConnected && knopf.getClientRects().length > 0) return;
		e.preventDefault();
		const ziel = ersatz?.() ?? document.querySelector<HTMLElement>('main');
		ziel?.focus({ preventScroll: true });
	}
</script>

<Dialog.Root bind:open={offen}>
	<Dialog.Trigger>
		{#snippet child({ props })}
			<button {...props} bind:this={knopf} type="button" class="knopf" aria-label={beschriftung}>
				<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
					<path d="M4 7h16M4 12h16M4 17h16" />
				</svg>
			</button>
		{/snippet}
	</Dialog.Trigger>
	<Dialog.Portal>
		<Dialog.Overlay>
			{#snippet child({ props })}
				<div {...props} class="schleier"></div>
			{/snippet}
		</Dialog.Overlay>
		<Dialog.Content
			preventScroll={false}
			onInteractOutside={daneben}
			onCloseAutoFocus={fokusDanach}
		>
			{#snippet child({ props })}
				<div {...props} class="schublade">
					<div class="kopf">
						<Dialog.Title>
							{#snippet child({ props: t })}
								<h2 {...t}>{titel}</h2>
							{/snippet}
						</Dialog.Title>
						<Dialog.Close>
							{#snippet child({ props: z })}
								<button {...z} type="button" class="zu" aria-label="Schließen">
									<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
										<path d="M6 6l12 12M18 6L6 18" />
									</svg>
								</button>
							{/snippet}
						</Dialog.Close>
					</div>
					<div class="inhalt">{@render children()}</div>
				</div>
			{/snippet}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<style>
	.knopf,
	.zu {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex: none;
		width: 2.75rem;
		height: 2.75rem;
		padding: 0;
		border: 0;
		border-radius: 0.625rem;
		background: none;
		color: var(--text);
		cursor: pointer;
	}
	.knopf:hover,
	.zu:hover,
	.knopf[data-state='open'] {
		background: var(--accent-soft);
	}
	svg path {
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: round;
	}
	.schleier {
		position: fixed;
		inset: 0;
		z-index: 60;
		background: var(--overlay);
	}
	.schublade {
		position: fixed;
		top: 0;
		bottom: 0;
		left: 0;
		z-index: 61;
		display: flex;
		flex-direction: column;
		width: min(20rem, calc(100vw - 3rem));
		border-right: 1px solid var(--line);
		background: var(--card);
		color: var(--text);
		box-shadow: var(--shadow-gross);
		animation: herein 180ms ease-out;
	}
	.kopf {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		padding: 0.25rem 0.25rem 0.25rem 1rem;
		border-bottom: 1px solid var(--line);
	}
	h2 {
		margin: 0;
		font-size: var(--text-md);
		color: var(--muted);
	}
	.inhalt {
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
	}
	@keyframes herein {
		from {
			transform: translateX(-100%);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.schublade {
			animation: none;
		}
	}
</style>
