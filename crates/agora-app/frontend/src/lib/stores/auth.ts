import { writable, get } from 'svelte/store';
import { api } from '$lib/api';

const TOKEN_KEY = 'agora-token';
const USER_KEY = 'agora-user';

export interface AuthState {
	token: string | null;
	userId: string | null;
	loading: boolean;
}

function createAuthStore() {
	const stored = typeof localStorage !== 'undefined';
	const initialToken = stored ? localStorage.getItem(TOKEN_KEY) : null;
	const initialUser = stored ? localStorage.getItem(USER_KEY) : null;

	if (initialToken) {
		api.setToken(initialToken);
	}

	const { subscribe, set, update } = writable<AuthState>({
		token: initialToken,
		userId: initialUser,
		loading: false
	});

	function persist(token: string | null, userId: string | null) {
		if (typeof localStorage === 'undefined') return;
		if (token) {
			localStorage.setItem(TOKEN_KEY, token);
			localStorage.setItem(USER_KEY, userId ?? '');
		} else {
			localStorage.removeItem(TOKEN_KEY);
			localStorage.removeItem(USER_KEY);
		}
	}

	return {
		subscribe,
		get isAuthenticated() {
			return get({ subscribe }).token !== null;
		},
		async register(username: string, password: string) {
			update((s) => ({ ...s, loading: true }));
			try {
				const resp = await api.register(username, password);
				api.setToken(resp.access_token);
				persist(resp.access_token, resp.user_id);
				set({ token: resp.access_token, userId: resp.user_id, loading: false });
				return resp;
			} catch (e) {
				update((s) => ({ ...s, loading: false }));
				throw e;
			}
		},
		async login(username: string, password: string) {
			update((s) => ({ ...s, loading: true }));
			try {
				const resp = await api.login(username, password);
				api.setToken(resp.access_token);
				persist(resp.access_token, resp.user_id);
				set({ token: resp.access_token, userId: resp.user_id, loading: false });
				return resp;
			} catch (e) {
				update((s) => ({ ...s, loading: false }));
				throw e;
			}
		},
		async logout() {
			try {
				await api.logout();
			} catch {
				// server might be unreachable — still clear local state
			}
			api.setToken(null);
			persist(null, null);
			set({ token: null, userId: null, loading: false });
		}
	};
}

export const auth = createAuthStore();
