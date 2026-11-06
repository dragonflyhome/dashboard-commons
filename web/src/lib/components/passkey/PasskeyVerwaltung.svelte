<script lang="ts">
	/**
	 * Passkeys auf /schluessel (Etappe 7c): alle Passkeys mit Stand,
	 * Windows-Konto, Gerät, Zeiten und bei offenen Anträgen die
	 * Vergleichszahl. Freigeben (nach dem Vergleich der Zahl), ablehnen,
	 * widerrufen und „alle Sitzungen beenden“.
	 *
	 * Im Hauptkonto geht alles sofort. Mit Passkey nur, wenn die Anmeldung
	 * höchstens 5 Minuten alt ist; sonst fragt der Server nach einer frischen
	 * Bestätigung (Art `bestaetigung`), und hier steht der Knopf dafür.
	 *
	 * Seit der Kopfleiste, Fassung 2, auch auf /anmelden (angemeldet). Der
	 * Passkey der eigenen Sitzung hat keine Knöpfe; der Server lehnt
	 * Selbst-Entscheidungen ohnehin ab.
	 */
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { ApiError, fehlerText } from '#lib/api/client.ts';
	import { fmtDateTime } from '#lib/format.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Nachlader } from '#lib/nachlader.ts';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import {
		STATUS_WORT,
		anmelden,
		browserFehlerSatz,
		entscheiden,
		liste as ladeListe,
		sitzungenBeenden,
		vonWort,
		type Entscheidung
	} from '#lib/passkey.ts';
	import type { Bedeutung, PasskeyListe, PasskeyStatus, PasskeyZeile } from '$vertrag';

	interface Props {
		/** Für Tests: eigene Ladefunktion statt `/api/schluessel/passkeys`. */
		laden?: () => Promise<PasskeyListe>;
	}
	let { laden = () => ladeListe() }: Props = $props();

	let daten = $state<PasskeyListe | null>(null);
	let ladefehler = $state<string | null>(null);
	let fehler = $state<string | null>(null);
	let meldung = $state<string | null>(null);
	/** Welche Handlung gerade auf Bestätigung wartet (Kennung und Art), oder „alle“. */
	let frage = $state<{ id: string; was: Entscheidung } | 'alle' | null>(null);
	let beschaeftigt = $state(false);
	/** Die zuletzt versuchte Handlung, wenn der Server eine frische Bestätigung will. */
	let nachBestaetigung = $state<{ id: string; was: Entscheidung; zahl: string | null } | null>(
		null
	);

	async function neuLaden(): Promise<void> {
		try {
			daten = await laden();
			ladefehler = null;
		} catch (e) {
			ladefehler = fehlerText(e);
		}
	}

	function bedeutung(s: PasskeyStatus): Bedeutung {
		switch (s) {
			case 'registriert':
				return 'ok';
			case 'ausstehend':
				return 'wartet';
			case 'widerrufen':
			case 'abgelehnt':
				return 'problem';
			case 'abgelaufen':
				return 'neutral';
		}
	}

	const WAS_WORT: Record<Entscheidung, string> = {
		freigeben: 'freigegeben',
		ablehnen: 'abgelehnt',
		widerrufen: 'widerrufen'
	};

	async function ausfuehren(id: string, was: Entscheidung, zahl: string | null): Promise<void> {
		beschaeftigt = true;
		fehler = null;
		meldung = null;
		try {
			const p = await entscheiden(id, was, zahl);
			meldung = `„${p.name}“ ist ${WAS_WORT[was]}.`;
			nachBestaetigung = null;
			frage = null;
			await neuLaden();
		} catch (e) {
			if (e instanceof ApiError && e.art === 'bestaetigung') {
				nachBestaetigung = { id, was, zahl };
			}
			fehler = fehlerText(e);
		} finally {
			beschaeftigt = false;
		}
	}

	/** Frische Bestätigung mit Passkey, danach die Handlung noch einmal. */
	async function bestaetigen(): Promise<void> {
		const offen = nachBestaetigung;
		beschaeftigt = true;
		fehler = null;
		try {
			await anmelden();
		} catch (e) {
			fehler = e instanceof DOMException ? browserFehlerSatz(e) : fehlerText(e);
			beschaeftigt = false;
			return;
		}
		beschaeftigt = false;
		await neuLaden();
		if (offen !== null) await ausfuehren(offen.id, offen.was, offen.zahl);
	}

	async function alleBeenden(): Promise<void> {
		beschaeftigt = true;
		fehler = null;
		try {
			await sitzungenBeenden();
			frage = null;
			meldung = 'Alle Sitzungen mit Passkey sind beendet.';
			await neuLaden();
		} catch (e) {
			fehler = fehlerText(e);
		} finally {
			beschaeftigt = false;
		}
	}

	function gefragt(p: PasskeyZeile, was: Entscheidung): boolean {
		return frage !== null && frage !== 'alle' && frage.id === p.id && frage.was === was;
	}

	onMount(() => {
		void neuLaden();
		// Ein Vorgang kommt meist mit `speicher`/`passkeys` 250 ms später:
		// zusammen eine Ladung (Nachlader). Ein Vorgang ohne das lädt nach dem Fenster.
		const nachlader = new Nachlader(() => void neuLaden());
		const off = live.onVorgang(() => {
			nachlader.spaeter();
		});
		// Live-Lücken: auch nach verpassten Ereignissen und bei jeder Schreibung der
		// Passkeys ohne Vorgang (Anmeldung mit Passkey, Sperren).
		const offNachholen = live.onNachholen(() => {
			nachlader.jetzt();
		});
		const offBereich = live.onBereich((b) => {
			if (b === 'passkeys') nachlader.jetzt();
		});
		return () => {
			nachlader.stop();
			off();
			offNachholen();
			offBereich();
		};
	});
</script>

<section class="card passkeys" aria-labelledby="h-passkeys">
	<h2 id="h-passkeys">Passkeys für andere Windows-Konten</h2>
	<p>
		Mit einem Passkey meldest du dich auch aus einem anderen Windows-Konto an. Neue Passkeys legt
		man auf <a href={resolve('/anmelden')}>Anmelden</a> an; aus einem anderen Konto wird das ein Antrag,
		der hier freigegeben werden muss.
	</p>

	<div aria-live="polite">
		{#if meldung}<p class="ok-box">{meldung}</p>{/if}
		{#if fehler}<p class="block-error" role="alert">{fehler}</p>{/if}
		{#if nachBestaetigung}
			<p>
				<button
					type="button"
					class="button primary"
					disabled={beschaeftigt}
					aria-busy={beschaeftigt}
					onclick={bestaetigen}>Mit Passkey bestätigen und noch einmal</button
				>
			</p>
		{/if}
	</div>

	{#if ladefehler}
		<p class="block-error">{ladefehler}</p>
	{:else if daten === null}
		<p class="muted">Lädt …</p>
	{:else}
		<p class="muted small">
			{#if daten.wie === 'hauptkonto'}
				Angemeldet im Hauptkonto: Freigeben, Ablehnen und Widerrufen gehen sofort.
			{:else if daten.wie === 'passkey'}
				Angemeldet mit Passkey: Freigeben, Ablehnen und Widerrufen gehen bis {fmtDateTime(
					daten.frisch_bis
				)} ohne neue Bestätigung.
			{/if}
			{daten.offen === 1 ? '1 offener Antrag.' : `${daten.offen} offene Anträge.`}
		</p>

		{#if daten.passkeys.length === 0}
			<p class="muted">Noch kein Passkey.</p>
		{:else}
			<ul class="plain liste">
				{#each daten.passkeys as p (p.id)}
					{@const eigener = daten.eigener === p.id}
					<li class="zeile" class:offen={p.status === 'ausstehend'}>
						<div class="kopf">
							<StatusDot bedeutung={bedeutung(p.status)} />
							<b>{p.name}</b>
							<span class="muted">· {STATUS_WORT[p.status]}</span>
							{#if eigener}<span class="pill">diese Sitzung</span>{/if}
						</div>
						<dl class="angaben small">
							<dt>Windows-Konto</dt>
							<dd>{p.konto}{p.hauptkonto ? ' (Hauptkonto)' : ''}</dd>
							<dt>Angelegt</dt>
							<dd>{fmtDateTime(p.angelegt)}</dd>
							<dt>Zuletzt benutzt</dt>
							<dd>{p.zuletzt_benutzt ? fmtDateTime(p.zuletzt_benutzt) : 'noch nie'}</dd>
							<dt>Verfahren</dt>
							<dd>{p.verfahren}</dd>
							{#if p.entschieden && p.status !== 'ausstehend'}
								<dt>Entschieden</dt>
								<dd>
									{fmtDateTime(p.entschieden)}{#if vonWort(p.entschieden_von)}, {vonWort(
											p.entschieden_von
										)}{/if}
								</dd>
							{/if}
							{#if p.ablauf}
								<dt>Antrag gilt bis</dt>
								<dd>{fmtDateTime(p.ablauf)}</dd>
							{/if}
						</dl>
						{#if eigener}
							<!-- Der Passkey dieser Sitzung: keine Knöpfe (kein Selbst-Bestätigen, kein Selbst-Widerruf; Kopfleiste, Fassung 2). -->
							<p class="muted small">Mit diesem Passkey bist du gerade angemeldet.</p>
						{:else if p.status === 'ausstehend' && p.vergleichszahl}
							<p class="vergleich">
								Vergleichszahl
								<span
									class="zahl"
									aria-label={`Vergleichszahl ${p.vergleichszahl.split('').join(' ')}`}
									>{p.vergleichszahl}</span
								>
							</p>
							{#if gefragt(p, 'freigeben')}
								<div
									class="frage"
									role="group"
									aria-label="Freigabe bestätigen"
									data-sperrt-neuladen
								>
									<p>
										Steht auf dem Bildschirm des anderen Kontos dieselbe Zahl <b
											>{p.vergleichszahl}</b
										>? Nur dann freigeben.
									</p>
									<button
										type="button"
										class="button primary"
										disabled={beschaeftigt}
										onclick={() => ausfuehren(p.id, 'freigeben', p.vergleichszahl)}
										>Ja, gleiche Zahl: freigeben</button
									>
									<button type="button" class="button" onclick={() => (frage = null)}
										>Abbrechen</button
									>
								</div>
							{:else}
								<p class="knoepfe">
									<button
										type="button"
										class="button primary"
										disabled={beschaeftigt}
										onclick={() => (frage = { id: p.id, was: 'freigeben' })}>Freigeben …</button
									>
									<button
										type="button"
										class="button"
										disabled={beschaeftigt}
										onclick={() => ausfuehren(p.id, 'ablehnen', null)}>Ablehnen</button
									>
								</p>
							{/if}
						{:else if p.status === 'registriert'}
							{#if gefragt(p, 'widerrufen')}
								<div
									class="frage"
									role="group"
									aria-label="Widerruf bestätigen"
									data-sperrt-neuladen
								>
									<p>
										„{p.name}“ widerrufen? Alle Sitzungen damit enden sofort. Das lässt sich nicht
										zurücknehmen.
									</p>
									<button
										type="button"
										class="button primary"
										disabled={beschaeftigt}
										onclick={() => ausfuehren(p.id, 'widerrufen', null)}>Ja, widerrufen</button
									>
									<button type="button" class="button" onclick={() => (frage = null)}
										>Abbrechen</button
									>
								</div>
							{:else}
								<p class="knoepfe">
									<button
										type="button"
										class="button"
										disabled={beschaeftigt}
										onclick={() => (frage = { id: p.id, was: 'widerrufen' })}>Widerrufen …</button
									>
								</p>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
		{/if}

		<h3>Sitzungen</h3>
		{#if frage === 'alle'}
			<div class="frage" role="group" aria-label="Alle Sitzungen beenden" data-sperrt-neuladen>
				<p>
					Alle Anmeldungen mit Passkey enden sofort, auch diese, falls sie eine ist. Das Hauptkonto
					bleibt angemeldet.
				</p>
				<button type="button" class="button primary" disabled={beschaeftigt} onclick={alleBeenden}
					>Ja, alle beenden</button
				>
				<button type="button" class="button" onclick={() => (frage = null)}>Abbrechen</button>
			</div>
		{:else}
			<p>
				<button
					type="button"
					class="button"
					disabled={beschaeftigt}
					onclick={() => (frage = 'alle')}>Alle Sitzungen mit Passkey beenden …</button
				>
			</p>
		{/if}
	{/if}
</section>

<style>
	.passkeys {
		margin-top: 18px;
	}
	.ok-box {
		background: var(--ok-soft);
		border-left: 4px solid var(--ok);
		padding: 8px 12px;
		border-radius: 10px;
	}
	.liste {
		display: grid;
		gap: 12px;
	}
	.zeile {
		border: 1px solid var(--line);
		border-radius: 12px;
		padding: 10px 12px;
	}
	.zeile.offen {
		border-color: var(--warn);
	}
	.kopf {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
	}
	.angaben {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 2px 12px;
		margin: 8px 0;
	}
	.angaben dt {
		color: var(--muted);
	}
	.angaben dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	.vergleich {
		display: flex;
		align-items: baseline;
		gap: 12px;
		flex-wrap: wrap;
	}
	.zahl {
		font-family: var(--mono);
		font-size: var(--text-5xl);
		font-weight: 700;
		letter-spacing: 0.25em;
	}
	.knoepfe,
	.frage {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		align-items: center;
	}
	.frage p {
		flex-basis: 100%;
		margin: 0;
	}
	@media (max-width: 26.25rem) {
		.angaben {
			grid-template-columns: 1fr;
		}
		.angaben dd {
			margin-bottom: 4px;
		}
	}
</style>
