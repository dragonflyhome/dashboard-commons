<!--
	Logo-Entwurf 1 „Blüte“ (Etappe 3b), vom Inhaber gewählt: das frühere
	Blüten-Symbol, flach und ohne Verläufe. static/logo.svg trägt dieselben
	Formen mit festen Farben; wer hier eine Form ändert, ändert sie dort mit. Fläche im Akzent, Blätter in der Farbe für Text auf dem
	Akzent, der Kern im Ton „wartet“. Nur Token (CSS-Variablen), kein Skript,
	keine Verweise nach außen; folgt hell und dunkel von selbst.
-->
<svg class="logo" viewBox="0 0 64 64" role="img" aria-label="Logo-Entwurf Blüte">
	<rect class="grund" x="0" y="0" width="64" height="64" rx="14" />
	<ellipse class="wasser" cx="32" cy="49" rx="21" ry="3.5" />
	<path class="blatt aussen" d="M32 47.5C15 46.5 7.5 37.5 8.5 27.5c9 2 17.5 10 23.5 20Z" />
	<path class="blatt aussen" d="M32 47.5c17-1 24.5-10 23.5-20-9 2-17.5 10-23.5 20Z" />
	<path class="blatt mitte" d="M32 47.5C21.5 42.5 15.5 30 19.5 18c8 5 12 17 12.5 29.5Z" />
	<path class="blatt mitte" d="M32 47.5c10.5-5 16.5-17.5 12.5-29.5-8 5-12 17-12.5 29.5Z" />
	<path class="blatt innen" d="M32 47.5c-7-9.5-7-24.5 0-34.5 7 10 7 25 0 34.5Z" />
	<circle class="kern" cx="32" cy="42" r="2.5" />
</svg>

<style>
	.logo {
		display: block;
		width: 100%;
		height: 100%;
	}
	.grund {
		fill: var(--accent);
	}
	.wasser {
		fill: var(--on-accent);
		opacity: 0.25;
	}
	.blatt {
		fill: var(--on-accent);
	}
	.aussen {
		opacity: 0.55;
	}
	.mitte {
		opacity: 0.78;
	}
	.kern {
		fill: var(--warn);
	}
</style>
