<script lang="ts">
	import { api } from '$lib/api';
	import { rooms, type Room } from '$lib/stores/rooms';

	interface Props {
		spaceId: string;
		onClose: () => void;
	}

	let { spaceId, onClose }: Props = $props();

	let space: Room | undefined = $state();
	let childRooms: Room[] = $state([]);
	let addRoomId = $state('');
	let error = $state('');
	let loading = $state(false);
	let allRooms = $state(new Map<string, Room>());

	rooms.subscribe((map) => {
		allRooms = map;
		space = map.get(spaceId);
		if (space?.children) {
			childRooms = space.children
				.map((id) => map.get(id))
				.filter((r): r is Room => r !== undefined);
		} else {
			childRooms = [];
		}
	});

	let avatarSrc = $derived(
		space?.avatarUrl ? api.downloadUrl(space.avatarUrl) : null
	);

	async function handleAddChild() {
		if (!addRoomId.trim()) return;
		error = '';
		loading = true;
		try {
			await api.setState(spaceId, 'm.space.child', addRoomId.trim(), {
				via: ['localhost']
			});
			addRoomId = '';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to add room';
		} finally {
			loading = false;
		}
	}

	async function handleRemoveChild(childId: string) {
		try {
			await api.setState(spaceId, 'm.space.child', childId, {});
		} catch (e) {
			console.error('Failed to remove child:', e);
		}
	}

	async function handleAvatarChange(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		try {
			const mxcUri = await api.uploadFile(file);
			await api.setState(spaceId, 'm.room.avatar', '', { url: mxcUri });
		} catch (e) {
			console.error('Failed to upload avatar:', e);
		}
		input.value = '';
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} role="dialog" tabindex="-1">
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<div class="modal" onclick={(e) => e.stopPropagation()} role="document">
		<h3>Space Settings</h3>

		<div class="avatar-section">
			{#if avatarSrc}
				<img class="current-avatar" src={avatarSrc} alt="Space icon" />
			{:else}
				<div class="avatar-placeholder">{space?.name?.charAt(0)?.toUpperCase() ?? '?'}</div>
			{/if}
			<div class="avatar-controls">
				<span class="space-name-display">{space?.name ?? '(unnamed)'}</span>
				<label class="btn-secondary upload-label">
					Change Icon
					<input type="file" accept="image/*" onchange={handleAvatarChange} style="display:none" />
				</label>
			</div>
		</div>

		{#if space?.topic}
			<p class="topic">{space.topic}</p>
		{/if}

		<div class="children-section">
			<h4>Child Rooms ({childRooms.length})</h4>
			{#if childRooms.length === 0}
				<p class="empty">No rooms in this space</p>
			{:else}
				{#each childRooms as child (child.id)}
					<div class="child-row">
						<span class="child-name">{child.name}</span>
						<button class="btn-danger remove-btn" onclick={() => handleRemoveChild(child.id)}>Remove</button>
					</div>
				{/each}
			{/if}
		</div>

		<form class="add-child-form" onsubmit={(e) => { e.preventDefault(); handleAddChild(); }}>
			<input
				type="text"
				bind:value={addRoomId}
				placeholder="!room_id:localhost"
			/>
			<button type="submit" class="btn-primary" disabled={loading || !addRoomId.trim()}>Add</button>
		</form>

		{#if error}
			<p class="error">{error}</p>
		{/if}

		<div class="actions">
			<button class="btn-secondary" onclick={onClose}>Close</button>
		</div>
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
		max-width: 440px;
		box-shadow: 0 8px 32px var(--shadow);
		max-height: 80vh;
		overflow-y: auto;
	}

	.modal h3 {
		font-size: 1.1rem;
		font-weight: 600;
		margin-bottom: 16px;
	}

	.avatar-section {
		display: flex;
		align-items: center;
		gap: 14px;
		margin-bottom: 16px;
	}

	.current-avatar {
		width: 56px;
		height: 56px;
		border-radius: 10px;
		object-fit: cover;
	}

	.avatar-placeholder {
		width: 56px;
		height: 56px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--accent);
		color: var(--accent-text);
		border-radius: 10px;
		font-size: 1.3rem;
		font-weight: 700;
	}

	.avatar-controls {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.space-name-display {
		font-weight: 600;
		font-size: 0.95rem;
	}

	.upload-label {
		display: inline-block;
		font-size: 0.7rem;
		padding: 4px 10px;
		cursor: pointer;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 6px;
		text-align: center;
	}

	.upload-label:hover {
		background: var(--surface-hover);
	}

	.topic {
		font-size: 0.8rem;
		color: var(--text-secondary);
		margin-bottom: 16px;
	}

	.children-section {
		margin-bottom: 12px;
	}

	.children-section h4 {
		font-size: 0.8rem;
		font-weight: 600;
		margin-bottom: 8px;
		color: var(--text-secondary);
	}

	.empty {
		font-size: 0.75rem;
		color: var(--text-muted);
	}

	.child-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 0;
		border-bottom: 1px solid var(--border);
	}

	.child-name {
		font-size: 0.8rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.remove-btn {
		font-size: 0.65rem;
		padding: 3px 8px;
		flex-shrink: 0;
	}

	.add-child-form {
		display: flex;
		gap: 8px;
		margin-bottom: 12px;
	}

	.add-child-form input {
		flex: 1;
		font-size: 0.8rem;
		padding: 8px 10px;
	}

	.add-child-form button {
		flex-shrink: 0;
	}

	.error {
		color: var(--danger);
		font-size: 0.8rem;
		margin-bottom: 8px;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		margin-top: 16px;
	}
</style>
