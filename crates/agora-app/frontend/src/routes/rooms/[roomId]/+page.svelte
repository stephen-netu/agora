<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type RoomEvent } from '$lib/api';
	import { auth } from '$lib/stores/auth';
	import { rooms, type Room } from '$lib/stores/rooms';
	import MessageList from '$lib/components/MessageList.svelte';
	import MessageInput from '$lib/components/MessageInput.svelte';

	let roomId = $derived(decodeURIComponent(page.params.roomId));

	let room: Room | undefined = $state();
	let messages: RoomEvent[] = $state([]);
	let sending = $state(false);
	let historyLoaded = $state(false);

	rooms.subscribe((map) => {
		room = map.get(roomId);
		messages = room?.timeline ?? [];
	});

	onMount(() => {
		loadHistory();
	});

	async function loadHistory() {
		if (historyLoaded) return;
		try {
			const resp = await api.getMessages(roomId, 50);
			rooms.appendMessages(roomId, resp.chunk);
			historyLoaded = true;
		} catch (e) {
			console.error('Failed to load history:', e);
		}
	}

	async function handleSend(text: string) {
		sending = true;
		try {
			await api.sendMessage(roomId, text);
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

			await api.sendEvent(roomId, 'm.room.message', {
				msgtype,
				body: file.name,
				url: mxcUri,
				info: {
					mimetype: file.type,
					size: file.size
				}
			});
		} catch (e) {
			console.error('Failed to upload file:', e);
		} finally {
			sending = false;
		}
	}

	let authState = $state({ token: null as string | null, userId: null as string | null, loading: false });
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
			<h2>{room?.name ?? '...'}</h2>
			{#if room?.topic}
				<span class="topic">{room.topic}</span>
			{/if}
		</div>
		<div class="header-actions">
			<button class="btn-secondary leave-btn" onclick={handleLeave}>Leave</button>
			<button class="btn-danger delete-btn" onclick={handleDelete} title="Delete (creator only)">Delete</button>
		</div>
	</div>

	<MessageList {messages} />
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
