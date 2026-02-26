<script lang="ts">
	import { tick } from 'svelte';
	import { api, type RoomEvent } from '$lib/api';
	import { auth } from '$lib/stores/auth';
	import { decryptEvent } from '$lib/crypto';
	import MediaMessage from './MediaMessage.svelte';

	interface Props {
		messages: RoomEvent[];
		encrypted?: boolean;
		pinnedEventIds?: string[];
		onPin?: (eventId: string) => void;
		onUnpin?: (eventId: string) => void;
	}

	let { messages, encrypted = false, pinnedEventIds = [], onPin, onUnpin }: Props = $props();

	function isPinned(eventId: string): boolean {
		return pinnedEventIds.includes(eventId);
	}

	let container: HTMLElement | undefined = $state();
	let authState = $state({ token: null as string | null, userId: null as string | null, deviceId: null as string | null, loading: false });
	auth.subscribe((v) => (authState = v));

	let decryptedCache = $state(new Map<string, { type: string; content: Record<string, unknown> } | null>());

	$effect(() => {
		for (const event of messages) {
			if (event.type === 'm.room.encrypted' && !decryptedCache.has(event.event_id)) {
				tryDecrypt(event);
			}
		}
	});

	async function tryDecrypt(event: RoomEvent) {
		const content = event.content;
		const senderKey = content.sender_key as string;
		const sessionId = content.session_id as string;
		const ciphertext = content.ciphertext as string;
		const roomId = event.room_id;

		if (!senderKey || !sessionId || !ciphertext) {
			decryptedCache.set(event.event_id, null);
			decryptedCache = new Map(decryptedCache);
			return;
		}

		const result = await decryptEvent(roomId, senderKey, sessionId, ciphertext);
		if (result) {
			decryptedCache.set(event.event_id, { type: result.type, content: result.content });
		} else {
			decryptedCache.set(event.event_id, null);
		}
		decryptedCache = new Map(decryptedCache);
	}

	function getDisplayEvent(event: RoomEvent): { type: string; content: Record<string, unknown> } {
		if (event.type === 'm.room.encrypted') {
			const cached = decryptedCache.get(event.event_id);
			if (cached) return cached;
			return { type: 'm.room.encrypted', content: { body: 'Unable to decrypt' } };
		}
		return { type: event.type, content: event.content };
	}

	function senderName(sender: string): string {
		const match = sender.match(/@([^:]+)/);
		return match ? match[1] : sender;
	}

	function formatTime(ts: number): string {
		const date = new Date(ts);
		return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
	}

	function isMediaMessage(display: { type: string; content: Record<string, unknown> }): boolean {
		const msgtype = display.content?.msgtype as string | undefined;
		return (
			display.type === 'm.room.message' &&
			(msgtype === 'm.image' || msgtype === 'm.file' || msgtype === 'm.audio' || msgtype === 'm.video')
		);
	}

	function isTextMessage(display: { type: string; content: Record<string, unknown> }): boolean {
		const msgtype = display.content?.msgtype as string | undefined;
		return (
			display.type === 'm.room.message' &&
			(msgtype === 'm.text' || msgtype === 'm.notice' || msgtype === 'm.emote')
		);
	}

	function isEncryptedUndecrypted(event: RoomEvent): boolean {
		if (event.type !== 'm.room.encrypted') return false;
		const cached = decryptedCache.get(event.event_id);
		return cached === null;
	}

	$effect(() => {
		if (messages.length && container) {
			tick().then(() => {
				container!.scrollTop = container!.scrollHeight;
			});
		}
	});
</script>

<div class="message-list" bind:this={container}>
	{#each messages as event (event.event_id)}
		{@const display = getDisplayEvent(event)}
		{#if isTextMessage(display) || isMediaMessage(display) || isEncryptedUndecrypted(event)}
			<div
				class="message"
				class:own={event.sender === authState.userId}
				class:encrypted-msg={event.type === 'm.room.encrypted'}
				class:pinned={isPinned(event.event_id)}
			>
				<div class="message-header">
					<span class="sender">{senderName(event.sender)}</span>
					{#if isPinned(event.event_id)}
						<span class="pin-badge" title="Pinned">&#128204;</span>
					{/if}
					{#if event.type === 'm.room.encrypted'}
						<span class="e2e-badge" title="End-to-end encrypted">&#128274;</span>
					{/if}
					<span class="time">{formatTime(event.origin_server_ts)}</span>
					<span class="msg-actions">
						{#if isPinned(event.event_id) && onUnpin}
							<button class="action-btn" onclick={() => onUnpin(event.event_id)} title="Unpin">&#128204;</button>
						{:else if onPin}
							<button class="action-btn" onclick={() => onPin(event.event_id)} title="Pin">&#128204;</button>
						{/if}
					</span>
				</div>
				{#if isEncryptedUndecrypted(event)}
					<div class="message-body undecryptable">Unable to decrypt</div>
				{:else if isMediaMessage(display)}
					<MediaMessage event={{ ...event, type: display.type, content: display.content }} />
				{:else}
					<div class="message-body">
						{display.content.body ?? ''}
					</div>
				{/if}
			</div>
		{:else if event.type === 'm.room.member'}
			<div class="system-message">
				{senderName(event.sender)}
				{event.content.membership === 'join' ? 'joined' : 'left'}
			</div>
		{/if}
	{/each}
</div>

<style>
	.message-list {
		flex: 1;
		overflow-y: auto;
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.message {
		max-width: 75%;
		padding: 8px 12px;
		background: var(--surface);
		border-radius: 12px;
		border-top-left-radius: 4px;
	}

	.message.own {
		align-self: flex-end;
		background: var(--message-own);
		border-top-left-radius: 12px;
		border-top-right-radius: 4px;
	}

	.message.pinned {
		border-left: 2px solid var(--accent);
	}

	.message-header {
		display: flex;
		align-items: baseline;
		gap: 8px;
		margin-bottom: 4px;
	}

	.sender {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--accent);
	}

	.pin-badge {
		font-size: 0.6rem;
	}

	.e2e-badge {
		font-size: 0.6rem;
	}

	.time {
		font-size: 0.65rem;
		color: var(--text-muted);
	}

	.msg-actions {
		opacity: 0;
		transition: opacity 0.1s;
		margin-left: auto;
	}

	.message:hover .msg-actions {
		opacity: 1;
	}

	.action-btn {
		background: none;
		border: none;
		font-size: 0.65rem;
		padding: 2px 4px;
		border-radius: 4px;
		color: var(--text-muted);
		cursor: pointer;
	}

	.action-btn:hover {
		background: var(--surface-hover);
		color: var(--accent);
	}

	.message-body {
		font-size: 0.875rem;
		line-height: 1.4;
		white-space: pre-wrap;
		word-wrap: break-word;
	}

	.undecryptable {
		font-style: italic;
		color: var(--text-muted);
	}

	.system-message {
		text-align: center;
		font-size: 0.75rem;
		color: var(--text-muted);
		padding: 4px 0;
	}
</style>
