<script lang="ts">
	/**
	 * Inhaltsverzeichnis für lange Projektseiten (Prüfbericht: eine
	 * Seite von über 20.000 px ohne Verzeichnis). Eine Seite besteht aus Abschnitten
	 * (`Abschnitt` aus dem Katalog: Karte mit h2). Dieses Verzeichnis liest die
	 * Überschriften der sichtbaren Abschnitte im Inhalt (`ziel`), sobald und
	 * solange die Seite sie zeichnet, und zeigt sie ab `ab` Abschnitten als
	 * aufklappbare Liste mit Sprunglinks. Die Projekt-Chats tun dafür nichts.
	 */
	import { untrack } from 'svelte';
	import { abschnitteLesen, type AbschnittEintrag } from './abschnitte.ts';

	interface Props {
		/** Der Ordner mit dem Inhalt der Seite. */
		ziel: HTMLElement | null;
		/** Ab so vielen Abschnitten erscheint das Verzeichnis. */
		ab?: number;
	}
	let { ziel, ab = 6 }: Props = $props();

	let eintraege = $state.raw<AbschnittEintrag[]>([]);

	$effect(() => {
		const el = ziel;
		if (el === null) return;
		const lesen = (): void => {
			const neu = abschnitteLesen(el);
			const alt = untrack(() => eintraege);
			// Nur bei einer echten Änderung neu setzen (der Beobachter meldet viel).
			if (JSON.stringify(neu) !== JSON.stringify(alt)) {
				eintraege = neu;
			}
		};
		lesen();
		// Abschnitte kommen nach dem Laden der Daten dazu oder fallen weg (leer).
		const beobachter = new MutationObserver(lesen);
		beobachter.observe(el, {
			childList: true,
			subtree: true,
			attributes: true,
			attributeFilter: ['hidden']
		});
		return () => {
			beobachter.disconnect();
		};
	});
</script>

{#if eintraege.length >= ab}
	<details class="card verzeichnis">
		<summary>Inhalt ({eintraege.length} Abschnitte)</summary>
		<nav aria-label="Inhalt dieser Seite">
			<ol>
				{#each eintraege as e (e.id)}
					<li><a href="#{e.id}">{e.titel}</a></li>
				{/each}
			</ol>
		</nav>
	</details>
{/if}

<style>
	.verzeichnis {
		margin: 12px 0;
	}
	.verzeichnis summary {
		cursor: pointer;
		font-weight: 600;
		padding: 0.3rem 0;
	}
	ol {
		margin: 0.4rem 0 0;
		padding-left: 1.4rem;
		columns: 2 16rem;
		column-gap: 1.5rem;
	}
	li {
		break-inside: avoid;
		padding: 0.15rem 0;
	}
</style>
