<script lang="ts">
	/**
	 * Eine Meldung zum Aufklappen in „Zuletzt“ (Etappe A3), Ziel von
	 * `#meldung-<id>`. Ungelesene tragen „neu“ und, mit Anmeldung, den Knopf
	 * „gelesen“; was der Knopf tut, entscheidet der Aufrufer (`ongelesen`).
	 */
	import type { Aktualisierung } from '$vertrag';
	import { ART_ZEICHEN } from '#lib/anstehend.ts';
	import { kurzZeit } from '#lib/unterdashboard.ts';
	import { meldungsWort } from './worte.ts';

	interface Props {
		meldung: Pick<
			Aktualisierung,
			'id' | 'art' | 'titel' | 'zeit' | 'anzahl' | 'behoben' | 'gelesen' | 'text'
		>;
		/** „gelesen“ anbieten (nur mit Anmeldung). */
		knopf?: boolean;
		/** Gerade läuft ein „gelesen“: alle Knöpfe warten. */
		gesperrt?: boolean;
		ongelesen?: ((id: string, knopf: HTMLButtonElement) => void) | undefined;
	}
	let { meldung: a, knopf = false, gesperrt = false, ongelesen }: Props = $props();
</script>

<details class="eintrag" id="meldung-{a.id}">
	<summary
		><span class="zeichen" aria-hidden="true">{ART_ZEICHEN.meldung}</span>
		<span class="titel">{a.titel}</span>{#if !a.gelesen}<span class="neu">neu</span>{/if}</summary
	>
	<ul class="plain einzel small">
		<li>
			{meldungsWort(a.art)} · {kurzZeit(a.zeit)}{#if a.anzahl > 1}, {a.anzahl}-mal{/if}
		</li>
		{#if a.behoben}<li>behoben</li>{/if}
		{#if a.text}<li class="text">{a.text}</li>{/if}
	</ul>
	{#if !a.gelesen && knopf}
		<button
			type="button"
			class="button klein"
			disabled={gesperrt}
			aria-label="Gelesen: {a.titel}"
			onclick={(ev) => {
				ongelesen?.(a.id, ev.currentTarget);
			}}>gelesen</button
		>
	{/if}
</details>

<style>
	.eintrag {
		border-radius: 0.4rem;
	}
	summary {
		cursor: pointer;
	}
	.zeichen {
		color: var(--muted);
		margin-right: 0.2rem;
	}
	.einzel {
		margin: 0.25rem 0 0.4rem 1.2rem;
		display: grid;
		gap: 0.1rem;
		font-weight: 400;
	}
	.neu {
		margin-left: 0.4rem;
		padding: 0 0.4rem;
		border-radius: 999px;
		background: var(--accent-soft);
		font-size: var(--text-2xs);
		font-weight: 600;
	}
	.text {
		white-space: pre-line;
		overflow-wrap: anywhere;
	}
	.klein {
		padding: 0.15rem 0.65rem;
		font-size: var(--text-sm);
		min-height: 2.25rem;
		margin: 0 0 0.3rem 1.2rem;
		font-weight: 400;
	}
</style>
