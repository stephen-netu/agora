import { writable } from 'svelte/store';
import { api } from '$lib/api';
import { rooms } from './rooms';

export interface SyncState {
	running: boolean;
	nextBatch: string | null;
	error: string | null;
}

async function tauriInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
	if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke(cmd, args);
	}
	return null;
}

async function processCryptoSync(
	toDeviceEvents: unknown[],
	otkCounts: Record<string, number>
) {
	try {
		if (toDeviceEvents.length > 0) {
			await tauriInvoke('process_sync_crypto', {
				toDeviceEvents
			});
		}
		const needsUpload = (await tauriInvoke('needs_otk_upload', {
			serverCounts: otkCounts
		})) as boolean | null;

		if (needsUpload) {
			const currentCount = otkCounts['signed_curve25519'] ?? 0;
			const otks = (await tauriInvoke('generate_otks', {
				currentCount
			})) as Record<string, unknown> | null;
			if (otks && Object.keys(otks).length > 0) {
				await api.keysUpload({ one_time_keys: otks });
			}
		}
	} catch (e) {
		console.error('crypto sync error:', e);
	}
}

function createSyncStore() {
	const { subscribe, set, update } = writable<SyncState>({
		running: false,
		nextBatch: null,
		error: null
	});

	let abortController: AbortController | null = null;
	let loopRunning = false;

	async function loop() {
		loopRunning = true;
		let backoff = 1000;

		while (loopRunning) {
			try {
				let since: string | undefined;
				update((s) => {
					since = s.nextBatch ?? undefined;
					return s;
				});

				const syncResp = await api.sync(since, since ? 30000 : 0);

				rooms.processSyncResponse(syncResp);

				const toDeviceEvents = syncResp.to_device?.events ?? [];
				const otkCounts = syncResp.device_one_time_keys_count ?? {};
				processCryptoSync(toDeviceEvents, otkCounts);

				update((s) => ({
					...s,
					nextBatch: syncResp.next_batch,
					error: null
				}));

				backoff = 1000;
			} catch (e) {
				if (!loopRunning) break;

				const message = e instanceof Error ? e.message : String(e);
				update((s) => ({ ...s, error: message }));

				await new Promise((r) => setTimeout(r, backoff));
				backoff = Math.min(backoff * 2, 30000);
			}
		}
	}

	return {
		subscribe,
		start() {
			if (loopRunning) return;
			update((s) => ({ ...s, running: true, error: null }));
			loop();
		},
		stop() {
			loopRunning = false;
			if (abortController) {
				abortController.abort();
				abortController = null;
			}
			set({ running: false, nextBatch: null, error: null });
		}
	};
}

export const sync = createSyncStore();
