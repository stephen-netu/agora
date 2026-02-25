<script lang="ts">
	import { tick } from 'svelte';
	import { api, type RoomEvent } from '$lib/api';
	import { auth } from '$lib/stores/auth';
	import MediaMessage from './MediaMessage.svelte';

	interface Props {
		messages: RoomEvent[];
	}

	let { messages }: Props = $props();

	let container: HTMLElement | undefined = $state();
	let authState = $state({ token: null as string | null, userId: null as string | null, loading: false });
	auth.subscribe((v) => (authState = v));

	function senderName(sender: string): string {
		const match = sender.match(/@([^:]+)/);
		return match ? match[1] : sender;
	}

	function formatTime(ts: number): string {
		const date = new Date(ts);
		return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
	}

	function isMediaMessage(event: RoomEvent): boolean {
		const msgtype = event.content?.msgtype as string | undefined;
		return (
			event.type === 'm.room.message' &&
			(msgtype === 'm.image' || msgtype === 'm.file' || msgtype === 'm.audio' || msgtype === 'm.video')
		);
	}

	function isTextMessage(event: RoomEvent): boolean {
		const msgtype = event.content?.msgtype as string | undefined;
		return (
			event.type === 'm.room.message' &&
			(msgtype === 'm.text' || msgtype === 'm.notice' || msgtype === 'm.emote')
		);
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
		{#if isTextMessage(event) || isMediaMessage(event)}
			<div
				class="message"
				class:own={event.sender === authState.userId}
			>
				<div class="message-header">
					<span class="sender">{senderName(event.sender)}</span>
					<span class="time">{formatTime(event.origin_server_ts)}</span>
				</div>
				{#if isMediaMessage(event)}
					<MediaMessage {event} />
				{:else}
					<div class="message-body">
						{event.content.body ?? ''}
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

	.time {
		font-size: 0.65rem;
		color: var(--text-muted);
	}

	.message-body {
		font-size: 0.875rem;
		line-height: 1.4;
		white-space: pre-wrap;
		word-wrap: break-word;
	}

	.system-message {
		text-align: center;
		font-size: 0.75rem;
		color: var(--text-muted);
		padding: 4px 0;
	}
</style>
