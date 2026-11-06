// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	namespace App {
		/** `art`: festes Wort aus der Fehlerantwort des Servers, etwa `anmeldung`. */
		interface Error {
			art?: string;
		}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
