<script lang="ts">
	import { api } from '$lib/api';
	import { rooms, type Room } from '$lib/stores/rooms';
	import { goto } from '$app/navigation';

	interface Props {
		spaceId: string;
		onClose: () => void;
	}

	let { spaceId, onClose }: Props = $props();

	let mode: 'create' | 'existing' = $state('create');
	let name = $state('');
	let topic = $state('');
	let existingRoomId = $state('');
	let error = $state('');
	let loading = $state(false);

	let space: Room | undefined = $state();
	rooms.subscribe((map) => {
		space = map.get(spaceId);
	});

	async function handleCreateInSpace() {
		error = '';
		loading = true;
		try {
			const resp = await api.createRoom(name || undefined, topic || undefined);
			await api.setState(spaceId, 'm.space.child', resp.room_id, {
				via: ['localhost']
			});
			rooms.addRoom(resp.room_id, name || '(unnamed)');
			onClose();
			goto(`/rooms/${encodeURIComponent(resp.room_id)}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create room';
		} finally {
			loading = false;
		}
	}

	async function handleAddExisting() {
		error = '';
		loading = true;
		try {
			await api.setState(spaceId, 'm.space.child', existingRoomId.trim(), {
				via: ['localhost']
			});
			onClose();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to add room';
		} finally {
			loading = false;
		}
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} role="dialog" tabindex="-1">
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<div class="modal" onclick={(e) => e.stopPropagation()} role="document">
		<h3>Add Room to {space?.name ?? 'Space'}</h3>

		<div class="tabs">
			<button class="tab" class:active={mode === 'create'} onclick={() => (mode = 'create')}>Create New</button>
			<button class="tab" class:active={mode === 'existing'} onclick={() => (mode = 'existing')}>Add Existing</button>
		</div>

		{#if mode === 'create'}
			<form onsubmit={(e) => { e.preventDefault(); handleCreateInSpace(); }}>
				<div class="field">
					<label for="room-name">Room Name</label>
					<input id="room-name" type="text" bind:value={name} placeholder="general" />
				</div>
				<div class="field">
					<label for="room-topic">Topic (optional)</label>
					<input id="room-topic" type="text" bind:value={topic} placeholder="A channel topic" />
				</div>
				{#if error}<p class="error">{error}</p>{/if}
				<div class="actions">
					<button type="button" class="btn-secondary" onclick={onClose}>Cancel</button>
					<button type="submit" class="btn-primary" disabled={loading}>
						{loading ? 'Creating...' : 'Create & Add'}
					</button>
				</div>
			</form>
		{:else}
			<form onsubmit={(e) => { e.preventDefault(); handleAddExisting(); }}>
				<div class="field">
					<label for="existing-id">Room ID</label>
					<input id="existing-id" type="text" bind:value={existingRoomId} placeholder="!abc123:localhost" />
				</div>
				{#if error}<p class="error">{error}</p>{/if}
				<div class="actions">
					<button type="button" class="btn-secondary" onclick={onClose}>Cancel</button>
					<button type="submit" class="btn-primary" disabled={loading || !existingRoomId.trim()}>
						{loading ? 'Adding...' : 'Add to Space'}
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

	.modal h3 {
		font-size: 1.1rem;
		font-weight: 600;
		margin-bottom: 16px;
	}

	.tabs {
		display: flex;
		gap: 4px;
		margin-bottom: 16px;
	}

	.tab {
		flex: 1;
		padding: 8px;
		font-size: 0.8rem;
		font-weight: 500;
		background: var(--surface);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
	}

	.tab.active {
		background: var(--accent);
		color: var(--accent-text);
		border-color: var(--accent);
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
