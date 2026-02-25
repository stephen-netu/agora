import { writable, derived, get } from 'svelte/store';
import { api, type RoomEvent, type SyncResponse } from '$lib/api';

export interface Room {
	id: string;
	name: string;
	topic: string;
	timeline: RoomEvent[];
	roomType?: string;
	children?: string[];
	avatarUrl?: string;
}

function createRoomsStore() {
	const { subscribe, set, update } = writable<Map<string, Room>>(new Map());

	function roomName(events: RoomEvent[]): string {
		const nameEvent = events.find((e) => e.type === 'm.room.name');
		return (nameEvent?.content?.name as string) ?? '(unnamed)';
	}

	function roomTopic(events: RoomEvent[]): string {
		const topicEvent = events.find((e) => e.type === 'm.room.topic');
		return (topicEvent?.content?.topic as string) ?? '';
	}

	function roomType(events: RoomEvent[]): string | undefined {
		const createEvent = events.find((e) => e.type === 'm.room.create');
		return createEvent?.content?.type as string | undefined;
	}

	function spaceChildren(events: RoomEvent[]): string[] {
		return events
			.filter(
				(e) =>
					e.type === 'm.space.child' &&
					e.state_key &&
					e.content &&
					Object.keys(e.content).length > 0
			)
			.map((e) => e.state_key!)
			.filter(Boolean);
	}

	function avatarUrl(events: RoomEvent[]): string | undefined {
		const avatarEvent = events.find((e) => e.type === 'm.room.avatar');
		return avatarEvent?.content?.url as string | undefined;
	}

	return {
		subscribe,
		processSyncResponse(sync: SyncResponse) {
			update((rooms) => {
				const joined = sync.rooms.join ?? {};
				for (const [roomId, data] of Object.entries(joined)) {
					const existing = rooms.get(roomId);

					const stateEvents = data.state.events;
					const timelineEvents = data.timeline.events;
					const allStateForParsing = [
						...(existing ? [] : []),
						...stateEvents
					];

					if (existing) {
						const name =
							roomName(stateEvents) !== '(unnamed)'
								? roomName(stateEvents)
								: existing.name;
						const topic = roomTopic(stateEvents) || existing.topic;
						const rt = roomType(stateEvents) ?? existing.roomType;
						const children = spaceChildren(stateEvents).length > 0
							? spaceChildren(stateEvents)
							: existing.children;
						const avatar = avatarUrl(stateEvents) ?? existing.avatarUrl;
						rooms.set(roomId, {
							...existing,
							name,
							topic,
							roomType: rt,
							children,
							avatarUrl: avatar,
							timeline: [...existing.timeline, ...timelineEvents]
						});
					} else {
						rooms.set(roomId, {
							id: roomId,
							name: roomName(stateEvents),
							topic: roomTopic(stateEvents),
							roomType: roomType(stateEvents),
							children: spaceChildren(stateEvents),
							avatarUrl: avatarUrl(stateEvents),
							timeline: timelineEvents
						});
					}
				}
				return new Map(rooms);
			});
		},
		appendMessages(roomId: string, events: RoomEvent[]) {
			update((rooms) => {
				const room = rooms.get(roomId);
				if (room) {
					const existingIds = new Set(room.timeline.map((e) => e.event_id));
					const newEvents = events.filter((e) => !existingIds.has(e.event_id));
					rooms.set(roomId, {
						...room,
						timeline: [...newEvents.reverse(), ...room.timeline]
					});
				}
				return new Map(rooms);
			});
		},
		addRoom(roomId: string, name: string = '(unnamed)', roomType?: string) {
			update((rooms) => {
				if (!rooms.has(roomId)) {
					rooms.set(roomId, {
						id: roomId,
						name,
						topic: '',
						timeline: [],
						roomType
					});
				}
				return new Map(rooms);
			});
		},
		removeRoom(roomId: string) {
			update((rooms) => {
				rooms.delete(roomId);
				return new Map(rooms);
			});
		},
		clear() {
			set(new Map());
		},
		getRoom(roomId: string): Room | undefined {
			return get({ subscribe }).get(roomId);
		}
	};
}

export const rooms = createRoomsStore();

export const roomList = derived(rooms, ($rooms) =>
	Array.from($rooms.values()).sort((a, b) => a.name.localeCompare(b.name))
);

export const spaceList = derived(rooms, ($rooms) =>
	Array.from($rooms.values())
		.filter((r) => r.roomType === 'm.space')
		.sort((a, b) => a.name.localeCompare(b.name))
);

export const orphanRoomList = derived(rooms, ($rooms) => {
	const allChildren = new Set<string>();
	for (const r of $rooms.values()) {
		if (r.children) {
			for (const c of r.children) allChildren.add(c);
		}
	}
	return Array.from($rooms.values())
		.filter((r) => r.roomType !== 'm.space' && !allChildren.has(r.id))
		.sort((a, b) => a.name.localeCompare(b.name));
});
