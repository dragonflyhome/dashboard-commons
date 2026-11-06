/**
 * Stil-Prüfung für ganz web/src (Etappe 3a-2, Plan 3a [stylelint_3a_2]).
 *
 * Farben, Schatten und Schriften kommen nur aus den Token in src/app.css
 * (var(--…)). Erlaubt sind daneben nur transparent, inherit und currentColor.
 * Die einzige Ausnahme sind die Token-Definitionen selbst (:root und das
 * dunkle Thema in app.css); dort steht ein stylelint-disable mit Begründung.
 * Ein Abschalten der Prüfung braucht immer eine Begründung hinter „--“ in
 * derselben Zeile. Eine Regel für px-Werte kommt erst, wenn die Abstands-Token
 * vollständig sind.
 */

/** Wörter, die statt eines Tokens stehen dürfen (ohne Rücksicht auf Groß- und Kleinschreibung). */
const ERLAUBT = ['/^transparent$/i', '/^inherit$/i', '/^currentcolor$/i'];

/** @type {import('stylelint').Config} */
export default {
	plugins: ['stylelint-declaration-strict-value'],
	reportDescriptionlessDisables: true,
	reportNeedlessDisables: true,
	reportInvalidScopeDisables: true,
	overrides: [{ files: ['**/*.svelte'], customSyntax: 'postcss-html' }],
	rules: {
		// Keine freien Farben: kein Hex, keine Farbfunktionen, keine Farbnamen.
		// Groß- und Kleinschreibung zählt nicht (RGB() ist dasselbe wie rgb()).
		'color-no-hex': true,
		'color-named': 'never',
		'function-disallowed-list': ['/^(rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|device-cmyk)$/i'],
		// Nichts aus dem Netz, nichts außerhalb der eigenen Datei, kein Erzwingen.
		'at-rule-disallowed-list': ['import'],
		'function-url-scheme-disallowed-list': ['/^https?/i'],
		'function-url-no-scheme-relative': true,
		'selector-pseudo-class-disallowed-list': ['global'],
		'declaration-no-important': true,
		'keyframe-declaration-no-important': true,
		// Farb-, Rand-, Schatten- und Schrift-Werte nur als Token. Kurzformen wie
		// border, outline und background werden zerlegt; geprüft wird dann nur
		// ihre Farbe (border: 1px solid var(--line) ist richtig). Die Kurzform
		// font wird ganz geprüft: font: inherit geht, font: 14px Arial nicht.
		'scale-unlimited/declaration-strict-value': [
			['/color$/', 'fill', 'stroke', 'box-shadow', 'font', 'font-family', 'font-size'],
			{
				expandShorthand: true,
				// color-mix(…) mit Token ist erwünscht; freie Farbfunktionen
				// fängt function-disallowed-list. Schriften nur als Token.
				ignoreFunctions: { font: false, 'font-size': false, 'font-family': false },
				// Nur diese Wörter, wie im Zielbild. Nicht initial oder unset:
				// color: initial gibt im dunklen Thema schwarzen Text. none nur für
				// Schatten und Füllung.
				ignoreValues: {
					'': ERLAUBT,
					'box-shadow': [...ERLAUBT, '/^none$/i'],
					fill: [...ERLAUBT, '/^none$/i'],
					stroke: [...ERLAUBT, '/^none$/i']
				}
			}
		]
	}
};
