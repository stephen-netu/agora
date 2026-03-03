import { writable } from 'svelte/store';
import { defaultTheme, type ThemeId } from '$lib/themes';

const STORAGE_KEY = 'agora-theme';

function createThemeStore() {
	const stored =
		typeof localStorage !== 'undefined'
			? (localStorage.getItem(STORAGE_KEY) as ThemeId | null)
			: null;

	const { subscribe, set } = writable<ThemeId>(stored ?? defaultTheme);

	return {
		subscribe,
		set(value: ThemeId) {
			set(value);
			if (typeof localStorage !== 'undefined') {
				localStorage.setItem(STORAGE_KEY, value);
			}
			if (typeof document !== 'undefined') {
				document.documentElement.dataset.theme = value;
			}
		},
		initialize() {
			const saved =
				typeof localStorage !== 'undefined'
					? (localStorage.getItem(STORAGE_KEY) as ThemeId | null)
					: null;
			const theme = saved ?? defaultTheme;
			set(theme);
			if (typeof document !== 'undefined') {
				document.documentElement.dataset.theme = theme;
			}
		}
	};
}

export const theme = createThemeStore();
