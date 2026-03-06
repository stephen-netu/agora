<script lang="ts">
	interface Reaction {
		emoji: string;
		count: number;
		userIds: string[];
		userReacted: boolean;
	}

	interface Props {
		reactions: Reaction[];
		onReact: (emoji: string) => void;
		onRemove: (emoji: string) => void;
	}

	let { reactions, onReact, onRemove }: Props = $props();

	function senderName(userId: string): string {
		const match = userId.match(/@([^:]+)/);
		return match ? match[1] : userId;
	}

	function getTooltipText(reaction: Reaction): string {
		const names = reaction.userIds.map(senderName);
		if (names.length === 1) {
			return names[0];
		} else if (names.length === 2) {
			return `${names[0]} and ${names[1]}`;
		} else if (names.length <= 5) {
			return names.slice(0, -1).join(', ') + ' and ' + names[names.length - 1];
		} else {
			return `${names.slice(0, 5).join(', ')} and ${names.length - 5} others`;
		}
	}

	function handleClick(reaction: Reaction) {
		if (reaction.userReacted) {
			onRemove(reaction.emoji);
		} else {
			onReact(reaction.emoji);
		}
	}
</script>

{#if reactions.length > 0}
	<div class="reactions">
		{#each reactions as reaction (reaction.emoji)}
			<button
				class="reaction-chip"
				class:user-reacted={reaction.userReacted}
				onclick={() => handleClick(reaction)}
				title={getTooltipText(reaction)}
			>
				<span class="emoji">{reaction.emoji}</span>
				<span class="count">{reaction.count}</span>
			</button>
		{/each}
	</div>
{/if}

<style>
	.reactions {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		margin-top: 6px;
	}

	.reaction-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 8px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 12px;
		cursor: pointer;
		font-size: 0.75rem;
		transition: all 0.15s ease;
	}

	.reaction-chip:hover {
		background: var(--surface-hover);
		border-color: var(--accent);
	}

	.reaction-chip.user-reacted {
		background: var(--accent);
		border-color: var(--accent);
		color: white;
	}

	.reaction-chip.user-reacted:hover {
		opacity: 0.9;
	}

	.emoji {
		font-size: 0.875rem;
	}

	.count {
		font-weight: 500;
		font-size: 0.7rem;
	}
</style>
