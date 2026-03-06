<script lang="ts">
	import { fly } from 'svelte/transition';

	interface Props {
		onReact: (emoji: string) => void;
		onPin?: () => void;
		onReply?: () => void;
		onMore?: () => void;
		pinned?: boolean;
		canPin?: boolean;
	}

	let { onReact, onPin, onReply, onMore, pinned = false, canPin = true }: Props = $props();

	let showPicker = $state(false);

	// Common emoji reactions for quick selection
	const quickReactions = ['👍', '❤️', '😂', '😮', '🎉', '🔥'];

	function handleReact(emoji: string) {
		onReact(emoji);
		showPicker = false;
	}

	function togglePicker() {
		showPicker = !showPicker;
	}

	// Close picker when clicking outside
	function handleClickOutside(event: MouseEvent) {
		const target = event.target as HTMLElement;
		if (!target.closest('.hover-menu') && !target.closest('.emoji-picker')) {
			showPicker = false;
		}
	}

	$effect(() => {
		if (showPicker) {
			document.addEventListener('click', handleClickOutside);
			return () => document.removeEventListener('click', handleClickOutside);
		}
	});
</script>

<div class="hover-menu">
	<div class="quick-reactions">
		{#each quickReactions as emoji}
			<button class="quick-emoji" onclick={() => handleReact(emoji)} title="React with {emoji}">
				{emoji}
			</button>
		{/each}
	</div>

	<button class="menu-btn picker-btn" onclick={togglePicker} title="More reactions">
		<svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
			<path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm-3.5-9c.83 0 1.5-.67 1.5-1.5S9.33 8 8.5 8 7 8.67 7 9.5 7.67 11 8.5 11zm7 0c.83 0 1.5-.67 1.5-1.5S16.33 8 15.5 8 14 8.67 14 9.5s.67 1.5 1.5 1.5zm-3.5 6.5c2.33 0 4.31-1.46 5.11-3.5H6.89c.8 2.04 2.78 3.5 5.11 3.5z"/>
		</svg>
	</button>

	<div class="divider"></div>

	{#if canPin && onPin}
		<button class="menu-btn" onclick={onPin} title={pinned ? 'Unpin' : 'Pin'}>
			<svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
				{#if pinned}
					<path d="M16 12V4H17V2H7V4H8V12L6 14V16H11.2V22H12.8V16H18V14L16 12Z"/>
				{:else}
					<path d="M16 12V4H17V2H7V4H8V12L6 14V16H11.2V22H12.8V16H18V14L16 12ZM8.8 14L10 12.8V4H14V12.8L15.2 14H8.8Z"/>
				{/if}
			</svg>
		</button>
	{/if}

	{#if onReply}
		<button class="menu-btn" onclick={onReply} title="Reply">
			<svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
				<path d="M10 9V5l-7 7 7 7v-4.1c5 0 8.5 1.6 11 5.1-1-5-4-10-11-11z"/>
			</svg>
		</button>
	{/if}

	{#if onMore}
		<button class="menu-btn" onclick={onMore} title="More options">
			<svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
				<path d="M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z"/>
			</svg>
		</button>
	{/if}

	{#if showPicker}
		<div class="emoji-picker" transition:fly={{ y: -5, duration: 150 }}>
			<div class="picker-grid">
				{#each ['👍', '👎', '❤️', '😂', '😮', '😢', '😡', '🎉', '🔥', '👏', '🤔', '😍', '🤣', '🙏', '💯', '✨', '🚀', '💪', '👀', '🤷', '♥️', '😭', '😅', '🥳'] as emoji}
					<button class="picker-emoji" onclick={() => handleReact(emoji)}>
						{emoji}
					</button>
				{/each}
			</div>
		</div>
	{/if}
</div>

<style>
	.hover-menu {
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 2px 4px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
		position: relative;
	}

	.quick-reactions {
		display: flex;
		gap: 2px;
	}

	.quick-emoji {
		background: none;
		border: none;
		padding: 4px;
		cursor: pointer;
		font-size: 1rem;
		border-radius: 4px;
		transition: background 0.15s ease;
	}

	.quick-emoji:hover {
		background: var(--surface-hover);
	}

	.divider {
		width: 1px;
		height: 16px;
		background: var(--border);
		margin: 0 2px;
	}

	.menu-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		background: none;
		border: none;
		padding: 4px;
		cursor: pointer;
		color: var(--text-muted);
		border-radius: 4px;
		transition: all 0.15s ease;
	}

	.menu-btn:hover {
		background: var(--surface-hover);
		color: var(--accent);
	}

	.picker-btn {
		padding: 4px 2px;
	}

	.emoji-picker {
		position: absolute;
		bottom: 100%;
		left: 50%;
		transform: translateX(-50%);
		margin-bottom: 8px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 12px;
		padding: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
		z-index: 100;
	}

	.emoji-picker::after {
		content: '';
		position: absolute;
		top: 100%;
		left: 50%;
		transform: translateX(-50%);
		border: 6px solid transparent;
		border-top-color: var(--border);
	}

	.picker-grid {
		display: grid;
		grid-template-columns: repeat(8, 1fr);
		gap: 4px;
	}

	.picker-emoji {
		background: none;
		border: none;
		padding: 6px;
		cursor: pointer;
		font-size: 1.25rem;
		border-radius: 6px;
		transition: background 0.15s ease;
	}

	.picker-emoji:hover {
		background: var(--surface-hover);
	}
</style>
