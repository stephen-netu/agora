<script lang="ts">
	import { spaceList, orphanRoomList, rooms, invitedRooms, type Room, type InvitedRoomInfo } from '$lib/stores/rooms';
	import { api } from '$lib/api';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';

	let spaces: Room[] = $state([]);
	let orphanRooms: Room[] = $state([]);
	let allRoomsMap = $state(new Map<string, Room>());
	let invites: InvitedRoomInfo[] = $state([]);

	spaceList.subscribe((v) => (spaces = v));
	orphanRoomList.subscribe((v) => (orphanRooms = v));
	rooms.subscribe((v) => (allRoomsMap = v));
	invitedRooms.subscribe((v) => (invites = v));

	async function acceptInvite(roomId: string) {
		try {
			await api.joinRoom(roomId);
			invitedRooms.update((list) => list.filter((r) => r.id !== roomId));
			goto(`/rooms/${encodeURIComponent(roomId)}`);
		} catch (e) {
			console.error('Failed to accept invite:', e);
		}
	}

	async function declineInvite(roomId: string) {
		try {
			await api.leaveRoom(roomId);
			invitedRooms.update((list) => list.filter((r) => r.id !== roomId));
		} catch (e) {
			console.error('Failed to decline invite:', e);
		}
	}

	let expandedSpaces = $state(new Set<string>());

	interface Props {
		onCreateRoom: () => void;
		onCreateSpace: () => void;
		onJoinRoom: () => void;
		onSpaceSettings: (spaceId: string) => void;
		onAddRoomToSpace: (spaceId: string) => void;
	}

	let { onCreateRoom, onCreateSpace, onJoinRoom, onSpaceSettings, onAddRoomToSpace }: Props = $props();

	function toggleSpace(spaceId: string) {
		const next = new Set(expandedSpaces);
		if (next.has(spaceId)) {
			next.delete(spaceId);
		} else {
			next.add(spaceId);
		}
		expandedSpaces = next;
	}

	function childRooms(space: Room): Room[] {
		if (!space.children) return [];
		return space.children
			.map((id) => allRoomsMap.get(id))
			.filter((r): r is Room => r !== undefined)
			.sort((a, b) => a.name.localeCompare(b.name));
	}

	function isActive(roomId: string): boolean {
		return page.url?.pathname?.includes(encodeURIComponent(roomId)) ?? false;
	}

	function avatarSrc(room: Room): string | null {
		if (!room.avatarUrl) return null;
		return api.downloadUrl(room.avatarUrl);
	}
</script>

<div class="room-list">
	<div class="room-list-header">
		<h2>Rooms</h2>
		<div class="room-actions">
			<button class="btn-icon" onclick={onCreateSpace} title="Create space">S</button>
			<button class="btn-icon" onclick={onCreateRoom} title="Create room">+</button>
			<button class="btn-icon" onclick={onJoinRoom} title="Join room">#</button>
		</div>
	</div>

	<div class="rooms-scroll">
		{#if invites.length > 0}
			<div class="section-label">Invites</div>
			{#each invites as invite (invite.id)}
				<div class="invite-item">
					<div class="invite-info">
						<span class="invite-name">{invite.name}</span>
						<span class="invite-from">from {invite.inviter.replace(/@([^:]+).*/, '$1')}</span>
					</div>
					<div class="invite-actions">
						<button class="invite-accept" onclick={() => acceptInvite(invite.id)} title="Accept">&#10003;</button>
						<button class="invite-decline" onclick={() => declineInvite(invite.id)} title="Decline">&#10005;</button>
					</div>
				</div>
			{/each}
		{/if}

		{#if spaces.length === 0 && orphanRooms.length === 0 && invites.length === 0}
			<p class="empty">No rooms yet</p>
		{/if}

		{#if spaces.length > 0}
			<div class="section-label">Spaces</div>
			{#each spaces as space (space.id)}
				<div class="space-group">
					<div class="space-header">
						<button
							class="space-toggle"
							onclick={() => toggleSpace(space.id)}
						>
							<span class="chevron" class:expanded={expandedSpaces.has(space.id)}>&#9656;</span>
						</button>
						<a
							href="/rooms/{encodeURIComponent(space.id)}"
							class="space-link"
							class:active={isActive(space.id)}
						>
							{#if avatarSrc(space)}
								<img class="space-avatar-img" src={avatarSrc(space)} alt="" />
							{:else}
								<span class="space-avatar">{space.name.charAt(0).toUpperCase()}</span>
							{/if}
							<span class="space-name">{space.name}</span>
						</a>
					<button
						class="space-action-btn"
						onclick={() => onAddRoomToSpace(space.id)}
						title="Add room to space"
					>+</button>
					<button
						class="space-action-btn"
						onclick={() => onSpaceSettings(space.id)}
						title="Space settings"
					>&#9881;</button>
					</div>
					{#if expandedSpaces.has(space.id)}
						<div class="space-children">
							{#each childRooms(space) as child (child.id)}
								<a
									href="/rooms/{encodeURIComponent(child.id)}"
									class="channel-item"
									class:active={isActive(child.id)}
								>
									<span class="channel-hash">#</span>
									<span class="channel-name">{child.name}</span>
								</a>
							{/each}
							{#if childRooms(space).length === 0}
								<p class="empty-children">No rooms in this space</p>
							{/if}
						</div>
					{/if}
				</div>
			{/each}
		{/if}

		{#if orphanRooms.length > 0}
			<div class="section-label">Rooms</div>
			{#each orphanRooms as room (room.id)}
				<a
					href="/rooms/{encodeURIComponent(room.id)}"
					class="room-item"
					class:active={isActive(room.id)}
				>
					{#if avatarSrc(room)}
						<img class="room-avatar-img" src={avatarSrc(room)} alt="" />
					{:else}
						<span class="room-avatar">{room.name.charAt(0).toUpperCase()}</span>
					{/if}
					<div class="room-info">
						<span class="room-name">{room.name}</span>
						{#if room.topic}
							<span class="room-topic">{room.topic}</span>
						{/if}
					</div>
				</a>
			{/each}
		{/if}
	</div>
</div>

<style>
	.room-list {
		display: flex;
		flex-direction: column;
		height: 100%;
	}

	.room-list-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px;
		border-bottom: 1px solid var(--border);
	}

	.room-list-header h2 {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text);
	}

	.room-actions {
		display: flex;
		gap: 4px;
	}

	.btn-icon {
		width: 28px;
		height: 28px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		font-size: 1rem;
		font-weight: 700;
		background: var(--surface);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
	}

	.btn-icon:hover {
		background: var(--surface-hover);
		color: var(--accent);
	}

	.rooms-scroll {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
	}

	.section-label {
		font-size: 0.65rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-muted);
		padding: 12px 12px 4px;
	}

	.invite-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		border-radius: 8px;
		background: var(--surface);
		margin-bottom: 4px;
	}

	.invite-info {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.invite-name {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--text);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.invite-from {
		font-size: 0.65rem;
		color: var(--text-muted);
	}

	.invite-actions {
		display: flex;
		gap: 4px;
		flex-shrink: 0;
	}

	.invite-accept, .invite-decline {
		width: 24px;
		height: 24px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 0.75rem;
		background: var(--bg);
	}

	.invite-accept {
		color: var(--accent);
	}

	.invite-accept:hover {
		background: var(--accent);
		color: var(--accent-text);
		border-color: var(--accent);
	}

	.invite-decline {
		color: var(--text-muted);
	}

	.invite-decline:hover {
		background: var(--danger);
		color: white;
		border-color: var(--danger);
	}

	.empty {
		text-align: center;
		color: var(--text-muted);
		font-size: 0.8rem;
		padding: 24px 0;
	}

	.space-group {
		margin-bottom: 2px;
	}

	.space-header {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.space-toggle {
		width: 20px;
		height: 20px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		background: none;
		color: var(--text-muted);
		border: none;
		border-radius: 4px;
		font-size: 0.7rem;
		flex-shrink: 0;
	}

	.space-toggle:hover {
		background: var(--surface-hover);
		color: var(--text);
	}

	.chevron {
		display: inline-block;
		transition: transform 0.15s;
	}

	.chevron.expanded {
		transform: rotate(90deg);
	}

	.space-link {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: 1;
		padding: 6px 8px;
		border-radius: 6px;
		text-decoration: none;
		color: var(--text);
		min-width: 0;
		transition: background 0.1s;
	}

	.space-link:hover {
		background: var(--surface-hover);
	}

	.space-link.active {
		background: var(--surface);
	}

	.space-avatar {
		width: 28px;
		height: 28px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--accent);
		color: var(--accent-text);
		border-radius: 6px;
		font-size: 0.75rem;
		font-weight: 600;
		flex-shrink: 0;
	}

	.space-avatar-img {
		width: 28px;
		height: 28px;
		border-radius: 6px;
		object-fit: cover;
		flex-shrink: 0;
	}

	.space-name {
		font-size: 0.8rem;
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.space-action-btn {
		width: 24px;
		height: 24px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		background: none;
		color: var(--text-muted);
		border: none;
		border-radius: 4px;
		font-size: 0.75rem;
		font-weight: 700;
		flex-shrink: 0;
		opacity: 0;
		transition: opacity 0.1s;
	}

	.space-header:hover .space-action-btn {
		opacity: 1;
	}

	.space-action-btn:hover {
		background: var(--surface-hover);
		color: var(--accent);
	}

	.space-children {
		padding-left: 24px;
	}

	.empty-children {
		font-size: 0.7rem;
		color: var(--text-muted);
		padding: 4px 12px;
	}

	.channel-item {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 3px 10px;
		border-radius: 4px;
		text-decoration: none;
		color: var(--text-muted);
		font-size: 0.8rem;
		transition: background 0.1s, color 0.1s;
	}

	.channel-item:hover {
		background: var(--surface-hover);
		color: var(--text);
	}

	.channel-item.active {
		color: var(--text);
		background: var(--surface);
	}

	.channel-hash {
		font-weight: 700;
		font-size: 0.85rem;
		color: var(--text-muted);
		flex-shrink: 0;
		width: 14px;
		text-align: center;
	}

	.channel-item.active .channel-hash,
	.channel-item:hover .channel-hash {
		color: var(--accent);
	}

	.channel-name {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		font-weight: 500;
	}

	.room-item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		border-radius: 8px;
		text-decoration: none;
		color: var(--text);
		transition: background 0.1s;
	}

	.room-item:hover {
		background: var(--surface-hover);
	}

	.room-item.active {
		background: var(--surface);
		border-left: 3px solid var(--accent);
	}

	.room-avatar {
		width: 36px;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--accent);
		color: var(--accent-text);
		border-radius: 8px;
		font-size: 0.9rem;
		font-weight: 600;
		flex-shrink: 0;
	}

	.room-avatar.small {
		width: 28px;
		height: 28px;
		font-size: 0.75rem;
		border-radius: 6px;
	}

	.room-avatar-img {
		width: 36px;
		height: 36px;
		border-radius: 8px;
		object-fit: cover;
		flex-shrink: 0;
	}

	.room-info {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.room-name {
		font-size: 0.85rem;
		font-weight: 500;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.room-topic {
		font-size: 0.7rem;
		color: var(--text-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
