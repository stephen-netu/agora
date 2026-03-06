<script lang="ts">
	import { api } from '$lib/api';
	import { onMount, onDestroy } from 'svelte';

	interface PresenceInfo {
		presence: 'online' | 'unavailable' | 'offline';
		last_active_ago?: number;
		status_msg?: string;
		currently_active?: boolean;
	}

	interface Props {
		userId: string;
		size?: 'small' | 'medium' | 'large';
		showTooltip?: boolean;
	}

	let { userId, size = 'medium', showTooltip = true }: Props = $props();

	let presence: PresenceInfo = $state({ presence: 'offline' });
	let heartbeatInterval: ReturnType<typeof setInterval> | null = null;

	const sizeClasses = {
		small: 'presence-dot--small',
		medium: 'presence-dot--medium',
		large: 'presence-dot--large'
	};

	const statusText = {
		online: 'Online',
		unavailable: 'Away',
		offline: 'Offline'
	};

	async function fetchPresence() {
		try {
			const data = await api.getPresence(userId);
			presence = data;
		} catch (e) {
			// Keep existing presence on error
		}
	}

	async function sendHeartbeat() {
		try {
			await api.heartbeat();
		} catch (e) {
			// Ignore heartbeat errors
		}
	}

	onMount(() => {
		fetchPresence();
		// Fetch presence every 30 seconds
		const fetchInterval = setInterval(fetchPresence, 30000);
		
		// Send heartbeat every 2 minutes to keep online status
		heartbeatInterval = setInterval(sendHeartbeat, 120000);
		
		return () => {
			clearInterval(fetchInterval);
			if (heartbeatInterval) clearInterval(heartbeatInterval);
		};
	});

	onDestroy(() => {
		if (heartbeatInterval) clearInterval(heartbeatInterval);
	});
</script>

<span 
	class="presence-dot {sizeClasses[size]} presence-dot--{presence.presence}"
	title={showTooltip ? `${statusText[presence.presence]}${presence.status_msg ? `: ${presence.status_msg}` : ''}` : undefined}
></span>

<style>
	.presence-dot {
		display: inline-block;
		border-radius: 50%;
		border: 2px solid var(--bg);
		flex-shrink: 0;
	}

	.presence-dot--small {
		width: 8px;
		height: 8px;
	}

	.presence-dot--medium {
		width: 12px;
		height: 12px;
	}

	.presence-dot--large {
		width: 16px;
		height: 16px;
	}

	.presence-dot--online {
		background: var(--success, #22c55e);
	}

	.presence-dot--unavailable {
		background: var(--warning, #f59e0b);
	}

	.presence-dot--offline {
		background: var(--text-muted, #6b7280);
	}
</style>
