import { writable } from 'svelte/store';
import { api } from '$lib/api';
import { rooms } from './rooms';

export interface SyncState {
	running: boolean;
	nextBatch: string | null;
	error: string | null;
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
