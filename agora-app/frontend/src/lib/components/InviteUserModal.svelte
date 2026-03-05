<script lang="ts">
	import { api, ApiError } from '$lib/api';

	interface Props {
		roomId: string;
		onClose: () => void;
	}

	let { roomId, onClose }: Props = $props();

	let searchTerm = $state('');
	let searchResults: Array<{ user_id: string; display_name?: string }> = $state([]);
	let selectedUser = $state('');
	let error = $state('');
	let loading = $state(false);
	let success = $state(false);
	let searching = $state(false);
	let searchTimer: ReturnType<typeof setTimeout> | null = null;
	let closeTimer: ReturnType<typeof setTimeout> | null = null;
	let searchRequestId = 0;

	$effect(() => {
		const term = searchTerm;
		const requestId = ++searchRequestId;
		if (searchTimer) clearTimeout(searchTimer);
		if (term.length < 2) {
			searchResults = [];
			return;
		}
		searchTimer = setTimeout(async () => {
			searching = true;
			try {
				const resp = await api.searchUsers(term, 8);
				if (requestId === searchRequestId && term === searchTerm) {
					searchResults = resp.results;
				}
			} catch {
				if (requestId === searchRequestId) searchResults = [];
			} finally {
				if (requestId === searchRequestId) searching = false;
			}
		}, 300);
	});

	$effect(() => {
		return () => {
			if (closeTimer) clearTimeout(closeTimer);
		};
	});

	function selectUser(userId: string) {
		selectedUser = userId;
		searchTerm = userId;
		searchResults = [];
	}

	async function handleInvite() {
		const target = selectedUser || searchTerm.trim();
		if (!target) return;
		error = '';
		loading = true;
		try {
			await api.inviteUser(roomId, target);
			success = true;
			closeTimer = setTimeout(() => {
				onClose();
			}, 1500);
		} catch (e) {
			if (e instanceof ApiError) {
				error = e.message;
			} else if (e instanceof Error) {
				error = e.message;
			} else {
				error = 'Failed to invite user';
			}
		} finally {
			loading = false;
		}
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} role="dialog" tabindex="-1">
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<div class="modal" onclick={(e) => e.stopPropagation()} role="document">
		<div class="modal-header">
			<h3>Invite User</h3>
			<button class="close-btn" onclick={onClose}>&times;</button>
		</div>

		{#if success}
			<p class="success">Invited!</p>
		{:else}
			<form onsubmit={(e) => { e.preventDefault(); handleInvite(); }}>
				<div class="field">
					<label for="user-search">Search or enter user ID</label>
					<div class="search-wrap">
						<input
							id="user-search"
							type="text"
							bind:value={searchTerm}
							oninput={() => { selectedUser = ''; }}
							placeholder="@username:server or search by name"
							autocomplete="off"
						/>
						{#if searching}
							<span class="search-spinner">⟳</span>
						{/if}
					</div>
					{#if searchResults.length > 0}
						<ul class="results">
							{#each searchResults as r (r.user_id)}
								<li>
									<button type="button" class="result-item" onclick={() => selectUser(r.user_id)}>
										<span class="result-name">{r.display_name ?? r.user_id.replace(/@([^:]+).*/, '$1')}</span>
										<span class="result-id">{r.user_id}</span>
									</button>
								</li>
							{/each}
						</ul>
					{/if}
				</div>

				{#if error}
					<p class="error">{error}</p>
				{/if}

				<div class="actions">
					<button type="button" class="btn-secondary" onclick={onClose}>Cancel</button>
					<button type="submit" class="btn-primary" disabled={loading || (!selectedUser && !searchTerm.trim())}>
						{loading ? 'Inviting...' : 'Invite'}
					</button>
				</div>
			</form>
		{/if}
	</div>
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: var(--overlay);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}

	.modal {
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 24px;
		width: 100%;
		max-width: 420px;
		box-shadow: 0 8px 32px var(--shadow);
	}

	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 20px;
	}

	.modal-header h3 {
		font-size: 1.1rem;
		font-weight: 600;
	}

	.close-btn {
		width: 28px;
		height: 28px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		background: none;
		border: none;
		color: var(--text-secondary);
		font-size: 1.3rem;
		border-radius: 6px;
		cursor: pointer;
	}

	.close-btn:hover {
		background: var(--surface-hover);
		color: var(--text);
	}

	.field {
		margin-bottom: 14px;
		position: relative;
	}

	.field label {
		display: block;
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--text-secondary);
		margin-bottom: 6px;
	}

	.search-wrap {
		position: relative;
	}

	.search-wrap input {
		width: 100%;
		box-sizing: border-box;
		padding-right: 28px;
	}

	.search-spinner {
		position: absolute;
		right: 10px;
		top: 50%;
		transform: translateY(-50%);
		color: var(--text-muted);
		font-size: 0.9rem;
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: translateY(-50%) rotate(0deg); }
		to { transform: translateY(-50%) rotate(360deg); }
	}

	.results {
		list-style: none;
		padding: 0;
		margin: 4px 0 0;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		max-height: 200px;
		overflow-y: auto;
	}

	.result-item {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		width: 100%;
		padding: 8px 12px;
		background: none;
		border: none;
		text-align: left;
		cursor: pointer;
	}

	.result-item:hover {
		background: var(--surface-hover);
	}

	.result-name {
		font-size: 0.85rem;
		font-weight: 500;
		color: var(--text);
	}

	.result-id {
		font-size: 0.7rem;
		color: var(--text-muted);
		font-family: monospace;
	}

	.error {
		color: var(--danger);
		font-size: 0.8rem;
		margin-bottom: 12px;
	}

	.success {
		color: var(--accent);
		font-size: 1rem;
		font-weight: 600;
		text-align: center;
		padding: 20px 0;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 20px;
	}
</style>
