<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type RoomEvent } from '$lib/api';
	import { auth, type AuthState } from '$lib/stores/auth';
	import { rooms, type Room } from '$lib/stores/rooms';
	import { ensureRoomKeysShared, encryptMessage, appendSigchainAction } from '$lib/crypto';
	import MessageList from '$lib/components/MessageList.svelte';
	import MessageInput from '$lib/components/MessageInput.svelte';
	import InviteUserModal from '$lib/components/InviteUserModal.svelte';

	let roomId = $derived(decodeURIComponent(page.params.roomId));

	let allRooms = $state(new Map<string, Room>());
	rooms.subscribe((map) => { allRooms = map; });

	let room = $derived(allRooms.get(roomId));
	let messages: RoomEvent[] = $derived(room?.timeline ?? []);
	let sending = $state(false);
	let historyLoadedFor = $state('');

	$effect(() => {
		const rid = roomId;
		if (rid && rid !== historyLoadedFor) {
			loadHistory(rid);
		}
	});

	async function loadHistory(rid: string) {
		try {
			const resp = await api.getMessages(rid, 50);
			rooms.appendMessages(rid, resp.chunk);
			historyLoadedFor = rid;
		} catch (e) {
			console.error('Failed to load history:', e);
		}
	}

	async function handleSend(text: string) {
		sending = true;
		try {
			const baseContent = { msgtype: 'm.text', body: text };

			// Append a sigchain Action link for this outgoing message (non-fatal).
			const proof = await appendSigchainAction('m.room.message', roomId, baseContent);

			// Include sigchain_proof in the content if available, so verifiers
			// can cross-reference the Action link.
			const content: Record<string, unknown> = proof
				? { ...baseContent, sigchain_proof: proof }
				: { ...baseContent };

			if (room?.encrypted) {
				const userId = authState.userId ?? '';
				await ensureRoomKeysShared(roomId, [userId]);
				const encrypted = await encryptMessage(roomId, 'm.room.message', content);
				if (encrypted) {
					await api.sendEvent(roomId, 'm.room.encrypted', encrypted as unknown as Record<string, unknown>);
				}
			} else {
				await api.sendEvent(roomId, 'm.room.message', content);
			}
		} catch (e) {
			console.error('Failed to send message:', e);
		} finally {
			sending = false;
		}
	}

	async function handleFileUpload(file: File) {
		sending = true;
		try {
			const mxcUri = await api.uploadFile(file);
			const isImage = file.type.startsWith('image/');
			const msgtype = isImage ? 'm.image' : 'm.file';
			const baseContent: Record<string, unknown> = {
				msgtype,
				body: file.name,
				url: mxcUri,
				info: { mimetype: file.type, size: file.size }
			};

			// Append sigchain Action link for the file send (non-fatal).
			const proof = await appendSigchainAction('m.room.message', roomId, baseContent);
			const content: Record<string, unknown> = proof
				? { ...baseContent, sigchain_proof: proof }
				: { ...baseContent };

			if (room?.encrypted) {
				const userId = authState.userId ?? '';
				await ensureRoomKeysShared(roomId, [userId]);
				const encrypted = await encryptMessage(roomId, 'm.room.message', content);
				if (encrypted) {
					await api.sendEvent(roomId, 'm.room.encrypted', encrypted as unknown as Record<string, unknown>);
				}
			} else {
				await api.sendEvent(roomId, 'm.room.message', content);
			}
		} catch (e) {
			console.error('Failed to upload file:', e);
		} finally {
			sending = false;
		}
	}

	let authState: AuthState = $state({ token: null, userId: null, deviceId: null, loading: false });
	auth.subscribe((v) => (authState = v));

	let showInviteModal = $state(false);
	let showPinnedBar = $state(false);
	let pinnedIds = $derived(room?.pinnedEvents ?? []);
	let pinnedMessages = $derived(
		pinnedIds
			.map((id) => messages.find((m) => m.event_id === id))
			.filter((m): m is RoomEvent => m !== undefined)
	);

	async function handlePin(eventId: string) {
		const current = room?.pinnedEvents ?? [];
		if (current.includes(eventId)) return;
		try {
			await api.setState(roomId, 'm.room.pinned_events', '', {
				pinned: [...current, eventId]
			});
		} catch (e) {
			console.error('Failed to pin:', e);
		}
	}

	async function handleUnpin(eventId: string) {
		const current = room?.pinnedEvents ?? [];
		try {
			await api.setState(roomId, 'm.room.pinned_events', '', {
				pinned: current.filter((id) => id !== eventId)
			});
		} catch (e) {
			console.error('Failed to unpin:', e);
		}
	}

	async function handleLeave() {
		try {
			await api.leaveRoom(roomId);
			rooms.removeRoom(roomId);
			goto('/rooms');
		} catch (e) {
			console.error('Failed to leave room:', e);
		}
	}

	async function handleDelete() {
		if (!confirm(`Delete "${room?.name ?? roomId}" permanently? This cannot be undone.`)) return;
		try {
			await api.deleteRoom(roomId);
			rooms.removeRoom(roomId);
			goto('/rooms');
		} catch (e) {
			const msg = e instanceof Error ? e.message : 'Failed to delete';
			alert(msg);
		}
	}
</script>

<div class="chat-view">
	<div class="chat-header">
		<div class="chat-info">
			<h2>
				{#if room?.encrypted}<span class="lock-icon" title="End-to-end encrypted">&#128274;</span>{/if}
				{room?.name ?? '...'}
			</h2>
			{#if room?.topic}
				<span class="topic">{room.topic}</span>
			{/if}
		</div>
		<div class="header-actions">
			{#if pinnedIds.length > 0}
				<button
					class="btn-secondary pin-toggle"
					onclick={() => (showPinnedBar = !showPinnedBar)}
					title="{pinnedIds.length} pinned message(s)"
				>&#128204; {pinnedIds.length}</button>
			{/if}
			<button class="btn-secondary invite-btn" onclick={() => (showInviteModal = true)}>Invite</button>
			<button class="btn-secondary leave-btn" onclick={handleLeave}>Leave</button>
			<button class="btn-danger delete-btn" onclick={handleDelete} title="Delete (creator only)">Delete</button>
		</div>
	</div>

	{#if showPinnedBar && pinnedMessages.length > 0}
		<div class="pinned-bar">
			<span class="pinned-label">&#128204; Pinned</span>
			<div class="pinned-list">
				{#each pinnedMessages as pm (pm.event_id)}
					<div class="pinned-item">
						<span class="pinned-sender">{pm.sender.replace(/@([^:]+).*/, '$1')}</span>
						<span class="pinned-body">{pm.content?.body ?? '(media)'}</span>
						<button class="pinned-unpin" onclick={() => handleUnpin(pm.event_id)} title="Unpin">&times;</button>
					</div>
				{/each}
			</div>
		</div>
	{/if}

	<MessageList
		{messages}
		encrypted={room?.encrypted ?? false}
		pinnedEventIds={pinnedIds}
		onPin={handlePin}
		onUnpin={handleUnpin}
	/>
	<MessageInput onSend={handleSend} onFileUpload={handleFileUpload} disabled={sending} />
	{#if showInviteModal}
		<InviteUserModal {roomId} onClose={() => (showInviteModal = false)} />
	{/if}
</div>

<style>
	.chat-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg);
	}

	.chat-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg);
	}

	.chat-info {
		display: flex;
		flex-direction: column;
	}

	.chat-info h2 {
		font-size: 1rem;
		font-weight: 600;
	}

	.lock-icon {
		font-size: 0.8em;
		margin-right: 4px;
	}

	.topic {
		font-size: 0.75rem;
		color: var(--text-secondary);
	}

	.header-actions {
		display: flex;
		gap: 6px;
	}

	.pin-toggle {
		font-size: 0.75rem;
		padding: 6px 10px;
	}

	.invite-btn, .leave-btn, .delete-btn {
		font-size: 0.75rem;
		padding: 6px 12px;
	}

	.pinned-bar {
		padding: 8px 20px;
		background: var(--surface);
		border-bottom: 1px solid var(--border);
	}

	.pinned-label {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--text-secondary);
		display: block;
		margin-bottom: 4px;
	}

	.pinned-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
		max-height: 120px;
		overflow-y: auto;
	}

	.pinned-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 8px;
		background: var(--bg);
		border-radius: 6px;
		font-size: 0.75rem;
	}

	.pinned-sender {
		font-weight: 600;
		color: var(--accent);
		flex-shrink: 0;
	}

	.pinned-body {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
		color: var(--text-secondary);
	}

	.pinned-unpin {
		margin-left: auto;
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 0.9rem;
		padding: 0 4px;
		cursor: pointer;
		flex-shrink: 0;
	}

	.pinned-unpin:hover {
		color: var(--danger);
	}
</style>
