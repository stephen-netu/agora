export type ThemeId = 'light' | 'dark' | 'seraphim';

export interface ThemeMeta {
	id: ThemeId;
	label: string;
	description: string;
}

export const themes: ThemeMeta[] = [
	{ id: 'light', label: 'Light', description: 'Clean and bright' },
	{ id: 'dark', label: 'Dark', description: 'Easy on the eyes' },
	{ id: 'seraphim', label: 'Seraphim', description: 'Black & neon orange' }
];

export const defaultTheme: ThemeId = 'dark';
