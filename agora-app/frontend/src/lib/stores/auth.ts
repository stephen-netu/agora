import { writable, get } from 'svelte/store';
import { api } from '$lib/api';
import { initSigchain } from '$lib/crypto';

const TOKEN_KEY = 'agora-token';
const USER_KEY = 'agora-user';
const DEVICE_KEY = 'agora-device';

export interface AuthState {
	token: string | null;
	userId: string | null;
	deviceId: string | null;
	loading: boolean;
}

async function tauriInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
	if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke(cmd, args);
	}
	return null;
}

async function initCryptoAndUploadKeys(userId: string, deviceId: string) {
	try {
		const deviceKeys = (await tauriInvoke('init_crypto', {
			userId,
			deviceId
		})) as Record<string, unknown> | null;

		if (deviceKeys) {
			const uploadResp = await api.keysUpload({ device_keys: deviceKeys });
			const count =
				uploadResp.one_time_key_counts?.signed_curve25519 ?? 0;

			const otks = (await tauriInvoke('generate_otks', {
				currentCount: count
			})) as Record<string, unknown> | null;

			if (otks && Object.keys(otks).length > 0) {
				await api.keysUpload({ one_time_keys: otks });
			}
		}

	} catch (e) {
		console.error('E2EE init failed:', e);
	}

	// Initialise (or restore) the sigchain identity — kept in a separate
	// try-catch so a sigchain failure is correctly attributed and does not
	// suppress or mask an E2EE initialisation error.
	try {
		await initSigchain();
	} catch (e) {
		console.error('sigchain init failed:', e);
	}
}

function createAuthStore() {
	const stored = typeof localStorage !== 'undefined';
	const initialToken = stored ? localStorage.getItem(TOKEN_KEY) : null;
	const initialUser = stored ? localStorage.getItem(USER_KEY) : null;
	const initialDevice = stored ? localStorage.getItem(DEVICE_KEY) : null;

	if (initialToken) {
		api.setToken(initialToken);
	}

	const { subscribe, set, update } = writable<AuthState>({
		token: initialToken,
		userId: initialUser,
		deviceId: initialDevice,
		loading: false
	});

	function persist(
		token: string | null,
		userId: string | null,
		deviceId: string | null
	) {
		if (typeof localStorage === 'undefined') return;
		if (token) {
			localStorage.setItem(TOKEN_KEY, token);
			localStorage.setItem(USER_KEY, userId ?? '');
			localStorage.setItem(DEVICE_KEY, deviceId ?? '');
		} else {
			localStorage.removeItem(TOKEN_KEY);
			localStorage.removeItem(USER_KEY);
			localStorage.removeItem(DEVICE_KEY);
		}
	}

	if (initialToken && initialUser && initialDevice) {
		initCryptoAndUploadKeys(initialUser, initialDevice);
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
				persist(resp.access_token, resp.user_id, resp.device_id);
				set({
					token: resp.access_token,
					userId: resp.user_id,
					deviceId: resp.device_id,
					loading: false
				});
				await initCryptoAndUploadKeys(resp.user_id, resp.device_id);
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
				persist(resp.access_token, resp.user_id, resp.device_id);
				set({
					token: resp.access_token,
					userId: resp.user_id,
					deviceId: resp.device_id,
					loading: false
				});
				await initCryptoAndUploadKeys(resp.user_id, resp.device_id);
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
			persist(null, null, null);
			set({ token: null, userId: null, deviceId: null, loading: false });
		}
	};
}

export const auth = createAuthStore();
