<script lang="ts">
	/**
	 * Der Kasten „Für dich“ (Etappe 5a): offene Aufgaben mit Titel, Projekt
	 * und Chat, großem Link „Öffnen“ (neuer Tab) und/oder den Knöpfen der
	 * Antworten, sonst dem Haken „erledigt“. Frist heute oder überschritten in
	 * der Farbe für „Achtung“ (`--warn`), nach der Frist-Stufe des Servers
	 * (Etappe 3a+; um Mitternacht lädt `aufgaben` neu). Alles sind echte Links und Buttons,
	 * also mit Tab erreichbar und mit sichtbarem Fokus.
	 *
	 * Auf dem Link steht gut lesbar sein Ziel (Host in ASCII-Form, wie der
	 * Browser ihn aufruft, `linkZiel`), etwa „Öffnen ↗ account.example.org“:
	 * Jeder Chat kann jedes https-Ziel eintragen, der Inhaber soll vor dem Klick
	 * sehen, wohin es geht.
	 *
	 * Etappe 3b-2: Neben der Überschrift steht in Worten, was gezählt ist
	 * („3 offene Aufgaben“), nicht nur eine Zahl; Fragen an den Inhaber zählen hier
	 * nicht, sie stehen in „Jetzt für dich“ (auf der Startseite seit dem
	 * Prüfbericht darüber; dort zeigt der Kasten nur die ersten
	 * `hoechstens` Aufgaben offen, den Rest zum Aufklappen).
	 *
	 * Nach den Knöpfen (oder dem Haken) steht immer
	 * „Etwas anderes …“ (`EtwasAnderes`) für eigene Worte. Die Aufgabe bleibt
	 * danach offen, liegt aber „beim Chat“ (Nachtrag der Abnahme): Sie zählt
	 * nicht mehr „für dich“ und steht ohne Knöpfe in einer eigenen Liste „Beim
	 * Chat“ mit „Deine Antwort: …“, bis der Chat sie abschließt, zurückzieht
	 * oder neu stellt.
	 */
	import { resolve } from '$app/paths';
	import {
		aufgabenZahlWort,
		chatWort,
		fristAchtung,
		fristWort,
		linkZiel,
		sichererLink,
		type AufgabenStand
	} from '#lib/aufgaben.ts';
	import type { AufgabeAnsicht } from '$vertrag';
	import { haengtSatz } from '#lib/fristen.ts';
	import EdgeHinweis from '../EdgeHinweis.svelte';
	import EtwasAnderes from '../EtwasAnderes.svelte';
	import { ETWAS_ANDERES, aufgabeEntwurf } from '#lib/etwas-anderes.ts';

	interface Props {
		stand: AufgabenStand;
		aufgaben: AufgabeAnsicht[];
		fehler: string | null;
		/** Kennung, deren Haken gerade läuft (Knöpfe dann gesperrt). */
		beschaeftigt?: string | null;
		onhaken: (id: string, antwort: string | null) => void;
		/** „Etwas anderes …“: eigene Worte senden; gibt einen Satz zurück, wenn es nicht ging. */
		oneigene: (id: string, eigene: string) => Promise<string | null>;
		/** Überschrift; auf der Projektseite etwa „Aufgaben für dich“. */
		titel?: string;
		/** Projekt je Aufgabe nennen (auf der Projektseite nicht nötig). */
		mitProjekt?: boolean;
		/** Ohne offene Aufgaben ganz weglassen (Projektseite). */
		leerVerbergen?: boolean;
		/**
		 * So viele Aufgaben offen zeigen, der Rest unter „und N weitere“ zum
		 * Aufklappen (Startseite); ohne Angabe alle.
		 */
		hoechstens?: number;
	}
	let {
		stand,
		aufgaben,
		fehler,
		beschaeftigt = null,
		onhaken,
		oneigene,
		titel = 'Für dich',
		mitProjekt = true,
		leerVerbergen = false,
		hoechstens = Infinity
	}: Props = $props();

	const kennung = $props.id();
	/** Je Aufgabe: ist „Etwas anderes …“ offen? Dann nimmt die Reihe der Knöpfe die ganze Breite. */
	const breit = $state<Record<string, boolean>>({});
	/** Mit eigenen Worten beantwortet: liegt beim Chat, wartet nicht auf dich. */
	const beimChat = $derived(
		aufgaben.filter((a) => a.zustand === 'offen' && a.antwort === ETWAS_ANDERES)
	);
	const fuerDich = $derived(aufgaben.filter((a) => !beimChat.includes(a)));
	const teil = $derived({ vorn: fuerDich.slice(0, hoechstens), rest: fuerDich.slice(hoechstens) });
	const sichtbar = $derived(
		!leerVerbergen || (stand !== 'laedt' && (stand !== 'da' || aufgaben.length > 0))
	);
</script>

{#snippet zeile(a: AufgabeAnsicht)}
	{@const link = sichererLink(a.link)}
	{@const ziel = linkZiel(a.link)}
	{@const frei = beschaeftigt === null}
	<!-- Anker „aufgabe-<id>“: Ziel aus „Anstehend“ und dem Unter-Dashboard (Etappe A1). -->
	<li class="aufgabe" class:achtung={fristAchtung(a.frist_stufe)} id="aufgabe-{a.id}" tabindex="-1">
		<div class="text">
			<p class="titel">{a.titel}</p>
			<p class="muted small">
				{#if mitProjekt}
					<a href={resolve('/projects/[slug]', { slug: a.projekt })}>{a.projekt}</a> ·
				{/if}
				{chatWort(a.von)}
				{#if a.frist}
					· <span class="frist">{fristWort(a.frist, a.frist_stufe)}</span>
				{:else if haengtSatz(a)}
					<!-- Etappe B3: bewegliche Frist, deren Ereignis noch kein Datum hat. -->
					· <span class="frist">{haengtSatz(a)}</span>
				{/if}
			</p>
		</div>
		<div class="handlung" class:breit={breit[a.id] === true}>
			{#if link}
				<!-- eslint-disable svelte/no-navigation-without-resolve -- Ziel liegt außerhalb der App (vom Server geprüft: https oder localhost) -->
				<a
					class="button primary gross oeffnen"
					href={link}
					target="_blank"
					rel="noopener noreferrer"
					data-sveltekit-reload
				>
					<span>Öffnen <span aria-hidden="true">↗</span></span>
					{#if ziel}<span class="ziel">{ziel}</span>{/if}
					<span class="unsichtbar">(öffnet in neuem Tab): {a.titel}</span>
				</a>
				<!-- eslint-enable svelte/no-navigation-without-resolve -->
			{:else if a.hat_link}
				<span class="muted small">Der Link lässt sich nicht lesen.</span>
			{/if}
			{#if a.knopf}
				{#each a.knopf as k (k)}
					<button
						type="button"
						class="button gross"
						disabled={!frei}
						aria-label="{k}: {a.titel}"
						onclick={() => {
							onhaken(a.id, k);
						}}>{k}</button
					>
				{/each}
			{:else}
				<button
					type="button"
					class="button gross haken"
					disabled={!frei}
					aria-label="Erledigt: {a.titel}"
					onclick={() => {
						onhaken(a.id, null);
					}}><span aria-hidden="true">✓</span> erledigt</button
				>
			{/if}
			<EtwasAnderes
				entwurf={aufgabeEntwurf(a.id)}
				bezug={a.titel}
				gesperrt={!frei}
				gross
				senden={(eigene: string) => oneigene(a.id, eigene)}
				onumschalten={(o: boolean) => {
					breit[a.id] = o;
				}}
			/>
		</div>
	</li>
{/snippet}

{#if sichtbar}
	<section class="card fuer-dich" id="aufgaben-fuer-dich" aria-labelledby="{kennung}-titel">
		<h2 id="{kennung}-titel">
			{titel}
			{#if stand === 'da' && fuerDich.length > 0}
				<span class="count">{aufgabenZahlWort(fuerDich.length)}</span>
			{/if}
		</h2>
		{#if stand === 'laedt'}
			<p class="muted">Aufgaben werden geladen …</p>
		{:else if stand !== 'da'}
			<EdgeHinweis class="muted hinweis" satz={fehler ?? ''} />
		{:else}
			{#if fehler}
				<p class="block-error" role="alert">{fehler}</p>
			{/if}
			{#if fuerDich.length === 0}
				<!-- Etappe 3b: nur über Aufgaben; Fragen an dich stehen in „Jetzt für dich“. -->
				<p class="muted">Gerade keine offenen Aufgaben.</p>
			{:else}
				<ul class="plain liste">
					{#each teil.vorn as a (a.id)}
						{@render zeile(a)}
					{/each}
				</ul>
				{#if teil.rest.length > 0}
					<!-- Auf der Startseite nur die ersten; der Rest aufklappbar. -->
					<details class="mehr">
						<summary>
							und {teil.rest.length === 1
								? 'eine weitere Aufgabe'
								: `${String(teil.rest.length)} weitere Aufgaben`}
						</summary>
						<ul class="plain liste">
							{#each teil.rest as a (a.id)}
								{@render zeile(a)}
							{/each}
						</ul>
					</details>
				{/if}
			{/if}
			{#if beimChat.length > 0}
				<h3 class="beim-chat-titel">Beim Chat</h3>
				<p class="muted small">
					Hier hast du mit eigenen Worten geantwortet. Der Chat ist dran; die Aufgabe bleibt stehen,
					bis er sie abschließt oder neu stellt.
				</p>
				<ul class="plain liste">
					{#each beimChat as a (a.id)}
						<li class="aufgabe beim-chat" id="aufgabe-{a.id}" tabindex="-1">
							<div class="text">
								<p class="titel">{a.titel}</p>
								<p class="muted small">
									{#if mitProjekt}
										<a href={resolve('/projects/[slug]', { slug: a.projekt })}>{a.projekt}</a> ·
									{/if}
									beim Chat
								</p>
								{#if a.eigene}
									<p class="eigene">Deine Antwort: {a.eigene}</p>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</section>
{/if}

<style>
	.fuer-dich {
		margin-bottom: 18px;
	}
	.liste {
		display: grid;
		gap: 10px;
		margin: 0;
	}
	.mehr {
		margin-top: 10px;
	}
	.mehr > summary {
		cursor: pointer;
		padding: 0.6rem 0;
	}
	.mehr > .liste {
		margin-top: 6px;
	}
	.aufgabe {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 10px 16px;
		padding: 10px 12px;
		border: 1px solid var(--line);
		border-left: 5px solid var(--line);
		border-radius: 10px;
	}
	.aufgabe.achtung {
		border-left-color: var(--warn);
		background: color-mix(in srgb, var(--warn) 10%, transparent);
	}
	.achtung .frist {
		font-weight: 700;
		color: var(--text);
		background: color-mix(in srgb, var(--warn) 30%, transparent);
		padding: 0 6px;
		border-radius: 6px;
	}
	.text {
		flex: 1 1 260px;
		min-width: 0;
	}
	.titel {
		font-weight: 600;
		margin: 0 0 2px;
		overflow-wrap: anywhere;
	}
	.text .small {
		margin: 0;
	}
	.handlung {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	/* „Etwas anderes …“ offen: die Knöpfe nehmen die ganze Breite, das Feld steht darunter. */
	.handlung.breit {
		flex: 1 1 100%;
	}
	.beim-chat-titel {
		font-size: var(--text-base);
		margin: 14px 0 2px;
	}
	.beim-chat {
		border-style: dashed;
	}
	.eigene {
		margin: 4px 0 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.gross {
		min-height: 44px;
		padding: 9px 18px;
		font-weight: 600;
	}
	.oeffnen {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 2px 10px;
		max-width: 100%;
	}
	.ziel {
		font-family: var(--mono);
		font-size: var(--text-md);
		font-weight: 700;
		padding: 1px 6px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--on-accent) 18%, transparent);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	/* Gesperrt (Rand gestrichelt, ohne Deckkraft) regelt app.css; hier nur: es arbeitet. */
	.button:disabled {
		cursor: progress;
	}
	.unsichtbar {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}
	.count {
		display: inline-block;
		min-width: 1.6em;
		padding: 0 8px;
		border-radius: 999px;
		background: var(--accent);
		color: var(--on-accent);
		font-size: var(--text-xs);
		text-align: center;
		vertical-align: middle;
	}
	@media (max-width: 37.5rem) {
		.handlung,
		.handlung > * {
			flex: 1 1 auto;
		}
		.handlung > * {
			justify-content: center;
		}
	}
</style>
