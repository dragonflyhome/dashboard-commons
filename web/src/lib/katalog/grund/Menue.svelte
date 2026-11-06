<script lang="ts">
	/**
	 * Menü (Etappe 3b, Bits UI DropdownMenu): ein Knopf, der eine Liste von
	 * Links aufklappt, etwa „Mehr“ in der Kopfzeile. Öffnen mit Enter,
	 * Leertaste, Pfeil runter oder Klick; Pfeile gehen durch die Punkte,
	 * Anfangsbuchstaben springen, Enter folgt dem Link, Escape schließt und
	 * gibt den Fokus an den Knopf zurück. Knopf und Punkte sind mindestens
	 * 44 px hoch, der Fokus ist sichtbar.
	 *
	 *   <Menue beschriftung="Mehr" punkte={[{ text: 'Daten', href: '/daten' }]} />
	 *
	 * `href` kommt fertig (mit `resolve()` gebaut); `aktiv` markiert den Punkt
	 * der aktuellen Seite (aria-current), und dann auch den Knopf.
	 */
	import type { Snippet } from 'svelte';
	import { DropdownMenu } from 'bits-ui';
	import type { MenuePunkt } from './typen.ts';

	interface Props {
		beschriftung: string;
		punkte: readonly MenuePunkt[];
		/** Zusatz im Knopf hinter der Beschriftung (etwa eine Zahl). */
		zusatz?: Snippet;
		/** Offen von außen setzen: nur für Tests. */
		offen?: boolean;
	}
	let { beschriftung, punkte, zusatz, offen = $bindable(false) }: Props = $props();
	const hier = $derived(punkte.some((p) => p.aktiv === true));
</script>

<DropdownMenu.Root bind:open={offen}>
	<DropdownMenu.Trigger>
		{#snippet child({ props })}
			<button {...props} type="button" class="ausloeser" class:hier
				>{beschriftung}<span class="pfeil" aria-hidden="true">▾</span>{@render zusatz?.()}</button
			>
		{/snippet}
	</DropdownMenu.Trigger>
	<DropdownMenu.Portal>
		<!-- Ohne Sperre: preventScroll sperrt <body> mit
		     pointer-events: none und overflow: hidden. Folgte ein Punkt einem
		     Link, blieb die Sperre nach dem Seitenwechsel stehen und die ganze
		     Seite war tot. Ein Linkmenü braucht keine Sperre; Tastatur, Fokus
		     und Escape bleiben gleich. -->
		<DropdownMenu.Content sideOffset={4} collisionPadding={8} align="end" preventScroll={false}>
			{#snippet child({ wrapperProps, props, open })}
				{#if open}
					<div {...wrapperProps}>
						<div {...props} class="liste">
							{#each punkte as p (p.href)}
								<DropdownMenu.Item textValue={p.text}>
									{#snippet child({ props: punkt })}
										<!-- eslint-disable svelte/no-navigation-without-resolve -- href kommt fertig mit resolve() -->
										<a
											{...punkt}
											class="punkt"
											href={p.href}
											aria-current={p.aktiv === true ? 'page' : undefined}
											>{p.text}{#if p.zahl !== undefined && p.zahl.n > 0}<span
													class="zahl"
													aria-label="{p.zahl.n} {p.zahl.text}">{p.zahl.n}</span
												>{/if}</a
										>
										<!-- eslint-enable svelte/no-navigation-without-resolve -->
									{/snippet}
								</DropdownMenu.Item>
							{/each}
						</div>
					</div>
				{/if}
			{/snippet}
		</DropdownMenu.Content>
	</DropdownMenu.Portal>
</DropdownMenu.Root>

<style>
	.ausloeser {
		display: inline-flex;
		align-items: center;
		gap: 0.3em;
		min-height: 2.75rem;
		padding: 0.375rem 0.75rem;
		border: 0;
		border-radius: 999px;
		background: none;
		color: var(--text);
		font: inherit;
		white-space: nowrap;
		cursor: pointer;
	}
	.ausloeser:hover,
	.ausloeser[data-state='open'] {
		background: var(--accent-soft);
	}
	/* Die aktuelle Seite liegt im Menü: der Knopf zeigt das wie ein aktiver Reiter. */
	.ausloeser.hier {
		background: var(--accent);
		color: var(--on-accent);
	}
	.ausloeser.hier:focus-visible {
		outline-color: var(--on-accent);
		outline-offset: -0.1875rem;
	}
	.pfeil {
		font-size: var(--text-xs);
	}
	.liste {
		min-width: 12rem;
		max-width: calc(100vw - 1rem);
		padding: 0.25rem;
		border: 1px solid var(--line);
		border-radius: 10px;
		background: var(--card);
		color: var(--text);
		box-shadow: var(--shadow-gross);
		z-index: 50;
	}
	.punkt {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		min-height: 2.75rem;
		padding: 0.5rem 0.75rem;
		border-radius: 6px;
		color: var(--text);
		text-decoration: none;
	}
	.punkt[data-highlighted] {
		background: var(--accent-soft);
		outline: 2px solid var(--fokus);
		outline-offset: -2px;
	}
	.punkt[aria-current='page'] {
		font-weight: 700;
	}
	.zahl {
		display: inline-block;
		min-width: 1.5em;
		padding: 0 0.35em;
		border-radius: 999px;
		background: var(--warn);
		color: var(--on-warn);
		font-size: var(--text-2xs);
		font-weight: 700;
		line-height: 1.5;
		text-align: center;
	}
</style>
