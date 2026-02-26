<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type RoomEvent } from '$lib/api';
	import { auth, type AuthState } from '$lib/stores/auth';
	import { rooms, type Room } from '$lib/stores/rooms';
	import { ensureRoomKeysShared, encryptMessage } from '$lib/crypto';
	import MessageList from '$lib/components/MessageList.svelte';
	import MessageInput from '$lib/components/MessageInput.svelte';

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
			if (room?.encrypted) {
				const userId = authState.userId ?? '';
				await ensureRoomKeysShared(roomId, [userId]);
				const encrypted = await encryptMessage(roomId, 'm.room.message', {
					msgtype: 'm.text',
					body: text
				});
				if (encrypted) {
					await api.sendEvent(roomId, 'm.room.encrypted', encrypted as unknown as Record<string, unknown>);
				}
			} else {
				await api.sendMessage(roomId, text);
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
			const content = {
				msgtype,
				body: file.name,
				url: mxcUri,
				info: { mimetype: file.type, size: file.size }
			};

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
			<button class="btn-secondary leave-btn" onclick={handleLeave}>Leave</button>
			<button class="btn-danger delete-btn" onclick={handleDelete} title="Delete (creator only)">Delete</button>
		</div>
	</div>

	<MessageList {messages} encrypted={room?.encrypted ?? false} />
	<MessageInput onSend={handleSend} onFileUpload={handleFileUpload} disabled={sending} />
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

	.leave-btn, .delete-btn {
		font-size: 0.75rem;
		padding: 6px 12px;
	}
</style>
