<script lang="ts">
	/**
	 * Anmelden mit Passkey (Etappe 7c). Für den Inhaber in einem anderen
	 * Windows-Konto: Im Hauptkonto meldet Windows ihn von selbst an, hier geht
	 * es mit Windows Hello, Handy oder Sicherheitsschlüssel.
	 *
	 * Zwei Teile: mit einem registrierten Passkey anmelden, und einen Passkey
	 * anlegen. Aus einem anderen Konto wird das ein Antrag mit einer
	 * vierstelligen Vergleichszahl; der Inhaber gibt ihn im Hauptkonto unter
	 * „Schlüssel“ frei, wenn dort dieselbe Zahl steht. Diese Seite fragt dann
	 * alle 5 Sekunden nach, ob er freigegeben ist.
	 *
	 * Kopfleiste, Fassung 2 (`[fassung_2_passkey_wege]`): Das Schloss oben
	 * führt hierher (Sprungmarken `#mit-passkey-anmelden` und
	 * `#passkey-anlegen`). Solange eine Verbindung besteht, geht immer ein
	 * neuer Antrag, auch angemeldet. Angemeldet steht darunter die Liste der
	 * Passkeys zum Bestätigen und Widerrufen (dieselbe wie unter „Schlüssel“;
	 * der eigene hat keine Knöpfe). Ohne Verbindung keine Handlung, nur ein
	 * Hinweis.
	 */
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import { localhostAdresse } from '#lib/anmeldung.ts';
	import { fehlerText } from '#lib/api/client.ts';
	import { fmtDateTime } from '#lib/format.ts';
	import OldLink from '#lib/components/OldLink.svelte';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import PasskeyVerwaltung from '#lib/components/passkey/PasskeyVerwaltung.svelte';
	import { freierKanal } from '#lib/freikanal.svelte.ts';
	import { LIVE_TEXT, PASSKEY_ANKER, istLive } from '#lib/verbindung.ts';
	import {
		STATUS_WORT,
		abmelden,
		anmeldeArt,
		anlegen,
		anmelden,
		aufLocalhost,
		browserFehlerSatz,
		eigeneAntraege,
		stand as ladeStand,
		zielPruefen
	} from '#lib/passkey.ts';
	import type { AntragErgebnis, EigeneAntraege, PasskeyStand } from '$vertrag';

	const ziel = $derived(zielPruefen(page.url.searchParams.get('ziel')));
	const lokal = $derived(aufLocalhost(page.url.href));

	let stand = $state<PasskeyStand | null>(null);
	let eigene = $state<EigeneAntraege | null>(null);
	let antrag = $state<AntragErgebnis | null>(null);
	let geraet = $state('');
	/** Hotfix: Knopf ohne Namen gedrückt; der Satz unter dem Feld sagt es. */
	let nameFehlt = $state(false);
	let geraetFeld = $state<HTMLInputElement | null>(null);
	let beschaeftigt = $state<'anmelden' | 'anlegen' | 'abmelden' | null>(null);
	let meldung = $state<string | null>(null);
	let fehler = $state<string | null>(null);

	// Hauptkonto sicher erst aus den eigenen Anträgen (geschützt), siehe anmeldeArt.
	const wie = $derived(anmeldeArt(stand, eigene));
	const offen = $derived(eigene?.passkeys.find((p) => p.status === 'ausstehend') ?? null);
	const zahl = $derived(offen?.vergleichszahl ?? antrag?.vergleichszahl ?? null);
	// Kopfleiste, Fassung 2: Passkey-Handlungen nur bei einer Verbindung (freier Kanal).
	const verbindung = $derived(freierKanal.zustand);
	const verbunden = $derived(verbindung !== null && istLive(verbindung));
	const angemeldet = $derived(wie === 'hauptkonto' || wie === 'passkey');

	/** Fehler aus Browser (Abbruch, kein Passkey) oder Server, als Satz. */
	function satz(e: unknown): string {
		return e instanceof DOMException ? browserFehlerSatz(e) : fehlerText(e);
	}

	async function standLaden(): Promise<void> {
		try {
			stand = await ladeStand();
		} catch {
			stand = null;
		}
	}

	/** Die eigenen Passkeys; meldet bei Windows an (ohne Inhaber-Pflicht). Still bei Fehlern. */
	async function eigeneLaden(): Promise<void> {
		try {
			eigene = await eigeneAntraege();
		} catch {
			eigene = null;
		}
	}

	async function mitPasskey(): Promise<void> {
		beschaeftigt = 'anmelden';
		fehler = null;
		meldung = null;
		try {
			stand = await anmelden();
			meldung = 'Angemeldet. Es geht weiter …';
			window.location.assign(ziel);
		} catch (e) {
			fehler = satz(e);
		} finally {
			beschaeftigt = null;
		}
	}

	async function passkeyAnlegen(ev: SubmitEvent): Promise<void> {
		ev.preventDefault();
		if (geraet.trim() === '') {
			nameFehlt = true;
			geraetFeld?.focus();
			return;
		}
		beschaeftigt = 'anlegen';
		fehler = null;
		meldung = null;
		try {
			antrag = await anlegen(geraet.trim());
			meldung =
				antrag.status === 'registriert'
					? 'Der Passkey ist angelegt und registriert. Er geht jetzt auch in anderen Konten (Handy, Sicherheitsschlüssel).'
					: 'Der Antrag ist gestellt. Bitte im Hauptkonto freigeben.';
			geraet = '';
			await eigeneLaden();
		} catch (e) {
			fehler = satz(e);
		} finally {
			beschaeftigt = null;
		}
	}

	async function abmeldenKlick(): Promise<void> {
		beschaeftigt = 'abmelden';
		fehler = null;
		try {
			await abmelden();
			// Kopfleiste, Fassung 2: Zustand zurücksetzen, zum freien Kanal wechseln (Schloss zu).
			freierKanal.abgemeldet();
			meldung = 'Abgemeldet.';
			await standLaden();
		} catch (e) {
			fehler = satz(e);
		} finally {
			beschaeftigt = null;
		}
	}

	onMount(() => {
		// Erst die geschützte Abfrage: Sie bringt Edge dazu, die Verbindung bei Windows
		// anzumelden (nach einer 401). Danach erst der freie Stand, der die Anmeldung nur
		// auf einer schon angemeldeten Verbindung sieht (Auflage aus Hotfix 3a-1).
		void (async () => {
			await eigeneLaden();
			await standLaden();
		})();
		// Solange ein Antrag wartet: alle 5 Sekunden nachsehen, ob er entschieden ist.
		const takt = setInterval(() => {
			if (offen !== null) void eigeneLaden();
		}, 5000);
		return () => {
			clearInterval(takt);
		};
	});
</script>

<svelte:head>
	<title>{seitenTitel('Anmelden')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Anmelden</p>
<h1>Anmelden</h1>

{#if !lokal}
	<p class="hinweis" role="note">
		Passkeys gehen nur unter <b>localhost</b>, nicht unter 127.0.0.1.
		<OldLink href={localhostAdresse(page.url.href)}>Diese Seite über localhost öffnen</OldLink>
	</p>
{/if}

<div class="ansage" aria-live="polite">
	{#if meldung}<p class="ok-box">{meldung}</p>{/if}
	{#if fehler}<p class="block-error" role="alert">{fehler}</p>{/if}
</div>

{#if verbindung !== null && !verbunden}
	<p class="hinweis" role="note">
		{LIVE_TEXT[verbindung]}: Ohne Verbindung zum Dashboard geht hier gerade nichts. Die Seite
		versucht es selbst neu; sobald oben „live“ steht, geht es weiter.
	</p>
{:else if verbindung === null}
	<p class="muted">Verbindung wird geprüft …</p>
{:else}
	<section class="card" id={PASSKEY_ANKER.anmelden} aria-labelledby="h-anmelden">
		<h2 id="h-anmelden">Mit Passkey anmelden</h2>
		{#if wie === 'hauptkonto'}
			<p>
				<StatusDot bedeutung="ok" /> Du bist mit dem Hauptkonto angemeldet (Windows). Hier braucht es
				keinen Passkey.
			</p>
			<p class="knoepfe">
				<OldLink class="button primary" href={ziel}>Weiter</OldLink>
				<!-- Zum Ausprobieren eines neuen Passkeys; im Hauptkonto nie nötig. -->
				<button
					type="button"
					class="button"
					disabled={beschaeftigt !== null || !lokal}
					aria-busy={beschaeftigt === 'anmelden'}
					onclick={mitPasskey}>Zum Ausprobieren: Mit Passkey anmelden</button
				>
			</p>
		{:else if stand?.wie === 'passkey'}
			<p>
				<StatusDot bedeutung="ok" /> Angemeldet mit dem Passkey „{stand.passkey}“, höchstens bis
				{fmtDateTime(stand.sitzung_bis)}.
			</p>
			<p class="knoepfe">
				<OldLink class="button primary" href={ziel}>Weiter</OldLink>
				<button
					type="button"
					class="button"
					disabled={beschaeftigt !== null}
					aria-busy={beschaeftigt === 'abmelden'}
					onclick={abmeldenKlick}>Abmelden</button
				>
			</p>
		{:else}
			<p>
				Für dich in einem anderen Windows-Konto auf diesem PC: mit Windows Hello, dem Handy oder
				einem Sicherheitsschlüssel. Der Passkey muss vorher freigegeben sein.
			</p>
			{#if stand !== null && !stand.verfuegbar}
				<!-- Kein „siehe Protokoll“; der Inhaber liest kein Protokoll. -->
				<p class="block-error">
					Passkeys sind noch nicht eingeschaltet: Ein einmaliger Schritt als Administrator fehlt. Er
					steht als Aufgabe unter <a href={resolve('/')}>„Für dich“ auf der Startseite</a>.
				</p>
			{/if}
			<p>
				<button
					type="button"
					class="button primary"
					disabled={beschaeftigt !== null || !lokal}
					aria-busy={beschaeftigt === 'anmelden'}
					onclick={mitPasskey}
					>{beschaeftigt === 'anmelden'
						? 'Bitte am Gerät bestätigen …'
						: 'Mit Passkey anmelden'}</button
				>
			</p>
		{/if}
	</section>

	<section class="card" id={PASSKEY_ANKER.anlegen} aria-labelledby="h-anlegen">
		<h2 id="h-anlegen">Passkey anlegen</h2>
		{#if eigene?.hauptkonto}
			<p>
				Du bist im Hauptkonto. Ein Passkey auf dem Handy oder einem Sicherheitsschlüssel ist sofort
				registriert und geht dann auch in anderen Windows-Konten.
			</p>
		{:else}
			<p>
				Aus einem anderen Windows-Konto wird das ein <b>Antrag</b>. Er gilt 24 Stunden. Du gibst ihn
				im Hauptkonto unter „Schlüssel“ frei, wenn dort dieselbe Zahl steht wie hier.
			</p>
		{/if}
		{#if stand !== null && !stand.verfuegbar}
			<p class="block-error">
				Passkeys sind noch nicht eingeschaltet: Ein einmaliger Schritt als Administrator fehlt. Er
				steht als Aufgabe unter <a href={resolve('/')}>„Für dich“ auf der Startseite</a>.
			</p>
		{/if}

		{#if zahl && offen}
			<div class="vergleich" role="status">
				<p>Vergleichszahl für diesen Antrag:</p>
				<p class="zahl" aria-label={`Vergleichszahl ${zahl.split('').join(' ')}`}>{zahl}</p>
				<p class="muted small">
					Wartet auf Freigabe{#if offen.ablauf}, gilt bis {fmtDateTime(offen.ablauf)}{/if}. Diese
					Seite sieht alle 5 Sekunden nach.
				</p>
			</div>
		{:else}
			<form class="anlegen" onsubmit={passkeyAnlegen}>
				<label for="geraet">Name des Geräts</label>
				<div class="zeile">
					<input
						id="geraet"
						type="text"
						bind:value={geraet}
						bind:this={geraetFeld}
						maxlength="60"
						autocomplete="off"
						placeholder="etwa Handy oder Laptop"
						aria-invalid={nameFehlt ? 'true' : undefined}
						aria-describedby={nameFehlt ? 'geraet-fehlt' : undefined}
						oninput={() => (nameFehlt = false)}
					/>
					<!-- Der
					     Knopf war ohne Namen still gesperrt. Jetzt ist er immer klickbar und
					     sagt, was fehlt. -->
					<button
						type="submit"
						class="button primary"
						disabled={beschaeftigt !== null || !lokal}
						aria-busy={beschaeftigt === 'anlegen'}
						>{beschaeftigt === 'anlegen'
							? 'Bitte am Gerät bestätigen …'
							: 'Passkey anlegen'}</button
					>
				</div>
				{#if nameFehlt}
					<p id="geraet-fehlt" class="name-fehlt" role="alert">
						Bitte zuerst einen Namen für das Gerät eingeben, etwa „Handy“ oder „Laptop“.
					</p>
				{/if}
			</form>
		{/if}

		{#if eigene && eigene.passkeys.length > 0}
			<h3>Passkeys dieses Windows-Kontos ({eigene.konto})</h3>
			<ul class="plain eigene">
				{#each eigene.passkeys as p (p.id)}
					<li>
						<StatusDot
							bedeutung={p.status === 'registriert'
								? 'ok'
								: p.status === 'ausstehend'
									? 'wartet'
									: 'neutral'}
						/>
						<span
							><b>{p.name}</b> · {STATUS_WORT[p.status]} · angelegt {fmtDateTime(p.angelegt)}</span
						>
					</li>
				{/each}
			</ul>
			{#if eigene.passkeys.some((p) => p.status === 'registriert') && wie !== 'passkey' && wie !== 'hauptkonto'}
				<p class="muted small">Ein Passkey ist freigegeben: Jetzt oben „Mit Passkey anmelden“.</p>
			{/if}
		{/if}
	</section>

	{#if angemeldet}
		<!-- Angemeldet: alle Passkeys zum Bestätigen und Widerrufen (Regeln aus [passkey]). -->
		<PasskeyVerwaltung />
	{/if}
{/if}

<style>
	.name-fehlt {
		margin: 0.375rem 0 0;
		color: var(--bad-text);
	}
	.hinweis {
		background: var(--warn-soft);
		border-left: 4px solid var(--warn);
		padding: 8px 12px;
		border-radius: 10px;
	}
	.ok-box {
		background: var(--ok-soft);
		border-left: 4px solid var(--ok);
		padding: 8px 12px;
		border-radius: 10px;
	}
	.card {
		margin-bottom: 18px;
		max-width: 46rem;
	}
	.knoepfe,
	.zeile {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		align-items: center;
	}
	.anlegen label {
		display: block;
		margin-bottom: 4px;
	}
	.zeile input {
		flex: 1 1 12rem;
		min-width: 0;
	}
	.vergleich {
		text-align: center;
		padding: 12px;
		border: 1px solid var(--line);
		border-radius: 12px;
	}
	.zahl {
		font-family: var(--mono);
		font-size: var(--text-riesig);
		letter-spacing: 0.3em;
		margin: 0.2em 0;
		font-weight: 700;
	}
	.eigene li {
		display: flex;
		gap: 8px;
		align-items: baseline;
		padding: 4px 0;
	}
</style>
