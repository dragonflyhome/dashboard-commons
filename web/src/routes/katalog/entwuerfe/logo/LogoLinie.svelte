<!--
	Logo-Entwurf 2 „Linie“ (Etappe 3b): die Blüte als feine Linie ohne Fläche,
	in der Textfarbe (currentColor), der Kern im Akzent. Ruhig, passt neben
	Text; wirkt klein (Favicon) dünner. Nur Token, kein Skript, keine Verweise
	nach außen.
-->
<svg class="logo" viewBox="0 0 64 64" role="img" aria-label="Logo-Entwurf Linie">
	<path class="linie" d="M32 50c-7-10-7-26 0-37 7 11 7 27 0 37Z" />
	<path class="linie" d="M32 50C21 45 14 32 18 19c6 4 10.5 11 12 19" />
	<path class="linie" d="M32 50c11-5 18-18 14-31-6 4-10.5 11-12 19" />
	<path class="linie" d="M32 50C16 49.5 7 40 8 29c5 1 10 4 14 8.5" />
	<path class="linie" d="M32 50c16-.5 25-10 24-21-5 1-10 4-14 8.5" />
	<path class="linie wasser" d="M12 55.5h40" />
	<circle class="kern" cx="32" cy="44" r="3" />
</svg>

<style>
	.logo {
		display: block;
		width: 100%;
		height: 100%;
		color: var(--text);
	}
	.linie {
		fill: none;
		stroke: currentColor;
		stroke-width: 2.5;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.wasser {
		stroke: var(--accent);
	}
	.kern {
		fill: var(--accent);
	}
</style>
