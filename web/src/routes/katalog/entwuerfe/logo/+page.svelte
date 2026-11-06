<script lang="ts">
	/**
	 * Logo-Entwürfe (Etappe 3b, plan-3b.toml Antwort 1): drei Entwürfe, jeder
	 * hell und dunkel nebeneinander, groß und in Favicon-Größe. Der Inhaber hat
	 * über die Frage im Projekt „dashboard“ Entwurf 1 „Blüte“ gewählt; er steht
	 * mit festen Farben in static/logo.svg (seit 3b-2 die eine Datei für
	 * Kopfleiste, Favicon und Manifest). Die Seite bleibt zum Vergleich.
	 *
	 * Die Entwürfe sind Svelte-Komponenten nach den Asset-Regeln des Zielbilds:
	 * SVG ohne Skript und ohne Verweise nach außen, Farben nur über Token.
	 * Die Kästen mit data-thema-vorschau zeigen ein Thema fest (app.css).
	 */
	import { asset, resolve } from '$app/paths';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import LogoBluete from './LogoBluete.svelte';
	import LogoLinie from './LogoLinie.svelte';
	import LogoKreis from './LogoKreis.svelte';

	const ENTWUERFE = [
		{
			nr: 1,
			name: 'Blüte',
			satz: 'Das frühere Blüten-Symbol, flach und ohne Verläufe: Fläche im Akzent, Blätter hell darauf. Am nächsten an dem, was du kanntest.',
			logo: LogoBluete,
			gewaehlt: true
		},
		{
			nr: 2,
			name: 'Linie',
			satz: 'Die Blüte als feine Linie in der Textfarbe, der Kern im Akzent. Ruhig und leicht; klein wirkt sie dünner.',
			logo: LogoLinie,
			gewaehlt: false
		},
		{
			nr: 3,
			name: 'Kreis',
			satz: 'Eine volle Blüte auf rundem Grund, darunter eine Welle. Kräftig, auch klein gut zu erkennen.',
			logo: LogoKreis,
			gewaehlt: false
		}
	] as const;
	const THEMEN = [
		{ wert: 'hell', text: 'Hell' },
		{ wert: 'dunkel', text: 'Dunkel' }
	] as const;
</script>

<svelte:head>
	<title>{seitenTitel('Logo-Entwürfe')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/katalog')}>Katalog</a> › Logo-Entwürfe</p>
<h1>Logo: drei Entwürfe</h1>
<p class="einleitung">
	Jeder Entwurf steht hell und dunkel nebeneinander, groß und so klein wie im Browser-Reiter. Die
	Farben kommen aus dem Dashboard selbst und passen sich an. Gewählt ist Entwurf 1 „Blüte“; er ist
	jetzt das Logo in Kopfleiste, Browser-Reiter und App-Symbol:
	<img class="heute" src={asset('logo.svg')} alt="Gewähltes Logo: Blüte" width="24" height="24" />
</p>

<div class="entwuerfe">
	{#each ENTWUERFE as e (e.nr)}
		{@const Logo = e.logo}
		<figure class="card entwurf" aria-labelledby="logo-{e.nr}-name">
			<figcaption>
				<h2 id="logo-{e.nr}-name">
					Entwurf {e.nr}: {e.name}
					{#if e.gewaehlt}<span class="pill gewaehlt">Gewählt</span>{/if}
				</h2>
				<p class="muted">{e.satz}</p>
			</figcaption>
			<div class="tafeln">
				{#each THEMEN as t (t.wert)}
					<div class="tafel" data-thema-vorschau={t.wert}>
						<span class="thema small">{t.text}</span>
						<div class="gross"><Logo /></div>
						<div class="klein" aria-hidden="true"><Logo /></div>
					</div>
				{/each}
			</div>
		</figure>
	{/each}
</div>

<style>
	.einleitung {
		max-width: 45rem;
	}
	.heute {
		display: inline-block;
		width: 1.5rem;
		height: 1.5rem;
		vertical-align: middle;
		border-radius: 0.375rem;
	}
	.entwuerfe {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 20rem), 1fr));
		gap: 1rem;
		margin-top: 1rem;
	}
	.entwurf {
		margin: 0;
		display: grid;
		gap: 0.75rem;
		min-width: 0;
	}
	h2 {
		margin: 0 0 0.25rem;
	}
	.gewaehlt {
		margin-left: 0.5rem;
		vertical-align: middle;
		background: var(--accent-soft);
		color: var(--accent-text);
	}
	figcaption p {
		margin: 0;
	}
	.tafeln {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.5rem;
	}
	/* Ein Thema fest: Grund und Text aus den Token des Kastens (data-thema-vorschau). */
	.tafel {
		display: grid;
		justify-items: center;
		gap: 0.5rem;
		padding: 0.75rem 0.5rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--bg);
		color: var(--text);
	}
	.thema {
		color: var(--muted);
	}
	.gross {
		width: 6rem;
		height: 6rem;
	}
	.klein {
		width: 2rem;
		height: 2rem;
	}
</style>
