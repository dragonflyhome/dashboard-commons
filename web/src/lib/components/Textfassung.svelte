<script lang="ts">
	/**
	 * Etappe 3b: Textfassung eines Bild-Bausteins (Balken, Ring, Kurve,
	 * Streifen, Pfeile) als Tabelle mit denselben Zahlen; gebaut von den
	 * Funktionen in #lib/textfassung.ts. Vorlese-Programme lesen die Tabelle
	 * immer; sichtbar wird sie über den Knopf „Als Text“ (gedrückt: sichtbar).
	 * Breite Tabellen rollen in ihrem eigenen Rahmen, die Seite bleibt schmal.
	 *
	 *   <Textfassung fassung={ganttText(plan)} />
	 */
	import { scrollShadow } from '#lib/scrollschatten.ts';
	import type { Textfassung } from '#lib/textfassung.ts';

	interface Props {
		fassung: Textfassung;
		/** Gleich sichtbar (Katalog). */
		offen?: boolean;
		/**
		 * Ohne Knopf, nur für Vorlese-Programme (oder mit `offen` sichtbar): wenn
		 * alles Wichtige schon als Text neben dem Bild steht (Ring).
		 */
		umschalter?: boolean;
	}
	let { fassung, offen = $bindable(false), umschalter = true }: Props = $props();
	const id = $props.id();
</script>

<div class="textfassung" data-textfassung>
	{#if umschalter}
		<button
			type="button"
			class="umschalter"
			aria-pressed={offen}
			aria-controls="{id}-tabelle"
			onclick={() => (offen = !offen)}>Als Text</button
		>
	{/if}
	<div id="{id}-tabelle" class={offen ? 'scroll-x sichtbar' : 'nur-vorlesen'} use:scrollShadow>
		<table>
			<caption>{fassung.titel}</caption>
			<thead>
				<tr>
					{#each fassung.spalten as s (s.schluessel)}
						<th scope="col" class:zahl={s.art === 'zahl'}>{s.titel}</th>
					{/each}
				</tr>
			</thead>
			<tbody>
				{#each fassung.zeilen as z, i (i)}
					<tr>
						{#each fassung.spalten as s, j (s.schluessel)}
							{#if j === 0}
								<th scope="row">{z[s.schluessel] ?? ''}</th>
							{:else}
								<td class:zahl={s.art === 'zahl'}>{z[s.schluessel] ?? ''}</td>
							{/if}
						{/each}
					</tr>
				{:else}
					<tr><td colspan={fassung.spalten.length}>{fassung.leer}</td></tr>
				{/each}
			</tbody>
		</table>
	</div>
</div>

<style>
	.textfassung {
		margin: 0.25rem 0 0.75rem;
	}
	.umschalter {
		min-height: 2rem;
		padding: 0.125rem 0.7rem;
		border: 1px solid var(--line);
		border-radius: 999px;
		background: var(--card);
		color: var(--accent);
		font: inherit;
		font-size: var(--text-xs);
		cursor: pointer;
	}
	.umschalter[aria-pressed='true'] {
		background: var(--accent-soft);
		color: var(--text);
	}
	.sichtbar {
		margin-top: 0.4rem;
	}
	table {
		margin: 0;
		font-size: var(--text-sm);
	}
	caption {
		text-align: left;
		color: var(--muted);
		font-size: var(--text-xs);
		padding-bottom: 0.25rem;
	}
	th[scope='row'] {
		color: var(--text);
		font-weight: 600;
		font-size: inherit;
	}
	.zahl {
		text-align: right;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
</style>
