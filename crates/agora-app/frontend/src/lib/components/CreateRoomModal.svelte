<script lang="ts">
	import { api } from '$lib/api';
	import { rooms } from '$lib/stores/rooms';
	import { goto } from '$app/navigation';

	interface Props {
		onClose: () => void;
	}

	let { onClose }: Props = $props();

	let name = $state('');
	let topic = $state('');
	let error = $state('');
	let loading = $state(false);

	async function handleCreate() {
		error = '';
		loading = true;
		try {
			const resp = await api.createRoom(name || undefined, topic || undefined);
			rooms.addRoom(resp.room_id, name || '(unnamed)');
			onClose();
			goto(`/rooms/${encodeURIComponent(resp.room_id)}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create room';
		} finally {
			loading = false;
		}
	}
</script>

<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} role="dialog" tabindex="-1">
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<div class="modal" onclick={(e) => e.stopPropagation()} role="document">
		<h3>Create Room</h3>

		<form onsubmit={(e) => { e.preventDefault(); handleCreate(); }}>
			<div class="field">
				<label for="room-name">Name</label>
				<input id="room-name" type="text" bind:value={name} placeholder="general" />
			</div>

			<div class="field">
				<label for="room-topic">Topic (optional)</label>
				<input id="room-topic" type="text" bind:value={topic} placeholder="A place to chat" />
			</div>

			{#if error}
				<p class="error">{error}</p>
			{/if}

			<div class="actions">
				<button type="button" class="btn-secondary" onclick={onClose}>Cancel</button>
				<button type="submit" class="btn-primary" disabled={loading}>
					{loading ? 'Creating...' : 'Create'}
				</button>
			</div>
		</form>
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
		max-width: 400px;
		box-shadow: 0 8px 32px var(--shadow);
	}

	.modal h3 {
		font-size: 1.1rem;
		font-weight: 600;
		margin-bottom: 20px;
	}

	.field {
		margin-bottom: 14px;
	}

	.field label {
		display: block;
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--text-secondary);
		margin-bottom: 6px;
	}

	.error {
		color: var(--danger);
		font-size: 0.8rem;
		margin-bottom: 12px;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 20px;
	}
</style>
