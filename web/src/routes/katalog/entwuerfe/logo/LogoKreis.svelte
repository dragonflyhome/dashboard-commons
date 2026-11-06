<!--
	Logo-Entwurf 3 „Kreis“ (Etappe 3b): eine volle Blüte im Akzent auf einem
	runden Grund in der ruhigen Akzentfläche, darunter eine Welle im Ton
	„läuft“. Kräftig auch klein (Favicon). Nur Token, kein Skript, keine
	Verweise nach außen.
-->
<svg class="logo" viewBox="0 0 64 64" role="img" aria-label="Logo-Entwurf Kreis">
	<circle class="grund" cx="32" cy="32" r="31" />
	<path class="bluete" d="M32 44c-6-8-6-20 0-29 6 9 6 21 0 29Z" />
	<path class="bluete" d="M31 44.5c-9-3-14.5-12-12.5-22 6.5 3 10.5 10.5 12.5 22Z" />
	<path class="bluete" d="M33 44.5c9-3 14.5-12 12.5-22-6.5 3-10.5 10.5-12.5 22Z" />
	<path class="bluete hinten" d="M30 45c-10 1-17-4-19.5-11.5 7.5-1 14 3 19.5 11.5Z" />
	<path class="bluete hinten" d="M34 45c10 1 17-4 19.5-11.5-7.5-1-14 3-19.5 11.5Z" />
	<path class="welle" d="M14 51c3-2 6-2 9 0s6 2 9 0 6-2 9 0 6 2 9 0" />
</svg>

<style>
	.logo {
		display: block;
		width: 100%;
		height: 100%;
	}
	.grund {
		fill: var(--accent-soft);
	}
	.bluete {
		fill: var(--accent);
	}
	.hinten {
		opacity: 0.6;
	}
	.welle {
		fill: none;
		stroke: var(--info);
		stroke-width: 2.5;
		stroke-linecap: round;
	}
</style>
