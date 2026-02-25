<script lang="ts">
	import { api, type RoomEvent } from '$lib/api';

	interface Props {
		event: RoomEvent;
	}

	let { event }: Props = $props();

	let msgtype = $derived(event.content.msgtype as string);
	let body = $derived((event.content.body as string) ?? 'file');
	let mxcUrl = $derived(event.content.url as string | undefined);
	let downloadUrl = $derived(mxcUrl ? api.downloadUrl(mxcUrl) : '');
	let mimetype = $derived((event.content.info as Record<string, unknown>)?.mimetype as string | undefined);
	let size = $derived((event.content.info as Record<string, unknown>)?.size as number | undefined);

	function formatSize(bytes?: number): string {
		if (!bytes) return '';
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}
</script>

{#if msgtype === 'm.image' && downloadUrl}
	<div class="media-image">
		<a href={downloadUrl} target="_blank" rel="noopener noreferrer">
			<img src={downloadUrl} alt={body} loading="lazy" />
		</a>
		<span class="caption">{body}</span>
	</div>
{:else if downloadUrl}
	<div class="media-file">
		<a href={downloadUrl} class="file-link" download={body}>
			<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
				<path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
				<polyline points="14,2 14,8 20,8" />
				<line x1="12" y1="18" x2="12" y2="12" />
				<polyline points="9,15 12,18 15,15" />
			</svg>
			<div class="file-info">
				<span class="file-name">{body}</span>
				<span class="file-meta">
					{#if mimetype}{mimetype}{/if}
					{#if size} &middot; {formatSize(size)}{/if}
				</span>
			</div>
		</a>
	</div>
{:else}
	<div class="message-body">{body}</div>
{/if}

<style>
	.media-image {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.media-image img {
		max-width: 320px;
		max-height: 240px;
		border-radius: 8px;
		object-fit: cover;
		cursor: pointer;
	}

	.caption {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.media-file {
		display: flex;
	}

	.file-link {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		background: var(--surface-hover);
		border-radius: 8px;
		text-decoration: none;
		color: var(--text);
		transition: background 0.1s;
	}

	.file-link:hover {
		background: var(--border);
	}

	.file-link svg {
		flex-shrink: 0;
		color: var(--accent);
	}

	.file-info {
		display: flex;
		flex-direction: column;
	}

	.file-name {
		font-size: 0.85rem;
		font-weight: 500;
	}

	.file-meta {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.message-body {
		font-size: 0.875rem;
		line-height: 1.4;
	}
</style>
