export interface AuthResponse {
	user_id: string;
	access_token: string;
	device_id: string;
}

export interface CreateRoomResponse {
	room_id: string;
}

export interface JoinRoomResponse {
	room_id: string;
}

export interface SendEventResponse {
	event_id: string;
}

export interface RoomEvent {
	event_id: string;
	room_id: string;
	sender: string;
	type: string;
	state_key?: string;
	content: Record<string, unknown>;
	origin_server_ts: number;
}

export interface MessagesResponse {
	start: string;
	end?: string;
	chunk: RoomEvent[];
}

export interface JoinedRoom {
	timeline: { events: RoomEvent[]; prev_batch?: string; limited: boolean };
	state: { events: RoomEvent[] };
}

export interface ToDeviceEvent {
	sender: string;
	type: string;
	content: Record<string, unknown>;
}

export interface SyncResponse {
	next_batch: string;
	rooms: {
		join?: Record<string, JoinedRoom>;
		invite?: Record<string, unknown>;
		leave?: Record<string, unknown>;
	};
	to_device?: { events: ToDeviceEvent[] };
	device_one_time_keys_count?: Record<string, number>;
}

export interface MediaUploadResponse {
	content_uri: string;
}

export interface HierarchyRoom {
	room_id: string;
	name?: string;
	topic?: string;
	num_joined_members: number;
	room_type?: string;
	children_state: RoomEvent[];
}

export interface HierarchyResponse {
	rooms: HierarchyRoom[];
}

export class ApiError extends Error {
	constructor(
		public status: number,
		public errcode: string,
		message: string
	) {
		super(message);
	}
}

const HOMESERVER_KEY = 'agora-homeserver';
const DEFAULT_HOMESERVER = 'http://localhost:8008';

export class AgoraApi {
	private baseUrl: string;
	private token: string | null = null;

	constructor(baseUrl?: string) {
		const stored =
			typeof localStorage !== 'undefined'
				? localStorage.getItem(HOMESERVER_KEY)
				: null;
		this.baseUrl = (baseUrl ?? stored ?? DEFAULT_HOMESERVER).replace(/\/+$/, '');
	}

	setBaseUrl(url: string) {
		this.baseUrl = url.replace(/\/+$/, '');
		if (typeof localStorage !== 'undefined') {
			localStorage.setItem(HOMESERVER_KEY, this.baseUrl);
		}
	}

	getBaseUrl(): string {
		return this.baseUrl;
	}

	setToken(token: string | null) {
		this.token = token;
	}

	private authHeaders(): Record<string, string> {
		if (!this.token) throw new Error('Not authenticated');
		return { Authorization: `Bearer ${this.token}` };
	}

	private async request<T>(
		method: string,
		path: string,
		options: {
			body?: unknown;
			headers?: Record<string, string>;
			query?: Record<string, string>;
			raw?: boolean;
		} = {}
	): Promise<T> {
		let url = `${this.baseUrl}${path}`;
		if (options.query) {
			const params = new URLSearchParams(options.query);
			url += `?${params}`;
		}

		const headers: Record<string, string> = { ...options.headers };
		let bodyData: BodyInit | undefined;

		if (options.body !== undefined && !options.raw) {
			headers['Content-Type'] = 'application/json';
			bodyData = JSON.stringify(options.body);
		} else if (options.raw && options.body) {
			bodyData = options.body as BodyInit;
		}

		const resp = await fetch(url, { method, headers, body: bodyData });

		if (!resp.ok) {
			let errcode = 'M_UNKNOWN';
			let error = `HTTP ${resp.status}`;
			try {
				const json = await resp.json();
				errcode = json.errcode ?? errcode;
				error = json.error ?? error;
			} catch {
				// non-JSON error body
			}
			throw new ApiError(resp.status, errcode, error);
		}

		return resp.json() as Promise<T>;
	}

	// ── Auth ──────────────────────────────────────────────────────

	async register(username: string, password: string): Promise<AuthResponse> {
		return this.request('POST', '/_matrix/client/v3/register', {
			body: { username, password }
		});
	}

	async login(username: string, password: string): Promise<AuthResponse> {
		return this.request('POST', '/_matrix/client/v3/login', {
			body: { type: 'm.login.password', user: username, password }
		});
	}

	async logout(): Promise<void> {
		await this.request('POST', '/_matrix/client/v3/logout', {
			headers: this.authHeaders()
		});
	}

	// ── Rooms ─────────────────────────────────────────────────────

	async createRoom(
		name?: string,
		topic?: string,
		creationContent?: Record<string, unknown>
	): Promise<CreateRoomResponse> {
		const body: Record<string, unknown> = {};
		if (name) body.name = name;
		if (topic) body.topic = topic;
		if (creationContent) body.creation_content = creationContent;
		return this.request('POST', '/_matrix/client/v3/createRoom', {
			headers: this.authHeaders(),
			body
		});
	}

	async createSpace(name?: string, topic?: string): Promise<CreateRoomResponse> {
		return this.createRoom(name, topic, { type: 'm.space' });
	}

	async joinRoom(roomIdOrAlias: string): Promise<JoinRoomResponse> {
		const encoded = encodeURIComponent(roomIdOrAlias);
		return this.request('POST', `/_matrix/client/v3/join/${encoded}`, {
			headers: this.authHeaders()
		});
	}

	async leaveRoom(roomId: string): Promise<void> {
		const encoded = encodeURIComponent(roomId);
		await this.request('POST', `/_matrix/client/v3/rooms/${encoded}/leave`, {
			headers: this.authHeaders()
		});
	}

	async deleteRoom(roomId: string): Promise<void> {
		const encoded = encodeURIComponent(roomId);
		await this.request('DELETE', `/_matrix/client/v3/rooms/${encoded}`, {
			headers: this.authHeaders()
		});
	}

	// ── State ─────────────────────────────────────────────────────

	async setState(
		roomId: string,
		eventType: string,
		stateKey: string,
		content: Record<string, unknown>
	): Promise<SendEventResponse> {
		const encoded = encodeURIComponent(roomId);
		const path = stateKey
			? `/_matrix/client/v3/rooms/${encoded}/state/${encodeURIComponent(eventType)}/${encodeURIComponent(stateKey)}`
			: `/_matrix/client/v3/rooms/${encoded}/state/${encodeURIComponent(eventType)}`;
		return this.request('PUT', path, {
			headers: this.authHeaders(),
			body: content
		});
	}

	// ── Hierarchy ─────────────────────────────────────────────────

	async getHierarchy(roomId: string): Promise<HierarchyResponse> {
		const encoded = encodeURIComponent(roomId);
		return this.request('GET', `/_matrix/client/v1/rooms/${encoded}/hierarchy`, {
			headers: this.authHeaders()
		});
	}

	// ── Events ────────────────────────────────────────────────────

	async sendMessage(roomId: string, body: string): Promise<SendEventResponse> {
		const encoded = encodeURIComponent(roomId);
		const txnId = crypto.randomUUID().replace(/-/g, '');
		return this.request(
			'PUT',
			`/_matrix/client/v3/rooms/${encoded}/send/m.room.message/${txnId}`,
			{
				headers: this.authHeaders(),
				body: { msgtype: 'm.text', body }
			}
		);
	}

	async sendEvent(
		roomId: string,
		eventType: string,
		content: Record<string, unknown>
	): Promise<SendEventResponse> {
		const encoded = encodeURIComponent(roomId);
		const txnId = crypto.randomUUID().replace(/-/g, '');
		return this.request(
			'PUT',
			`/_matrix/client/v3/rooms/${encoded}/send/${encodeURIComponent(eventType)}/${txnId}`,
			{
				headers: this.authHeaders(),
				body: content
			}
		);
	}

	async getMessages(roomId: string, limit: number = 50): Promise<MessagesResponse> {
		const encoded = encodeURIComponent(roomId);
		return this.request('GET', `/_matrix/client/v3/rooms/${encoded}/messages`, {
			headers: this.authHeaders(),
			query: { limit: String(limit), dir: 'b' }
		});
	}

	// ── Sync ──────────────────────────────────────────────────────

	async sync(since?: string, timeout: number = 30000): Promise<SyncResponse> {
		const query: Record<string, string> = { timeout: String(timeout) };
		if (since) query.since = since;
		return this.request('GET', '/_matrix/client/v3/sync', {
			headers: this.authHeaders(),
			query
		});
	}

	// ── Media ─────────────────────────────────────────────────────

	async uploadFile(file: File): Promise<string> {
		const query: Record<string, string> = {};
		if (file.name) query.filename = file.name;

		const headers = {
			...this.authHeaders(),
			'Content-Type': file.type || 'application/octet-stream'
		};

		const resp = await fetch(
			`${this.baseUrl}/_matrix/media/v3/upload${file.name ? `?filename=${encodeURIComponent(file.name)}` : ''}`,
			{
				method: 'POST',
				headers,
				body: file
			}
		);

		if (!resp.ok) {
			const json = await resp.json().catch(() => ({}));
			throw new ApiError(
				resp.status,
				(json as Record<string, string>).errcode ?? 'M_UNKNOWN',
				(json as Record<string, string>).error ?? `Upload failed: HTTP ${resp.status}`
			);
		}

		const result = (await resp.json()) as MediaUploadResponse;
		return result.content_uri;
	}

	downloadUrl(mxcUri: string): string {
		const stripped = mxcUri.replace(/^mxc:\/\//, '');
		return `${this.baseUrl}/_matrix/media/v3/download/${stripped}`;
	}

	// ── E2EE: Keys ────────────────────────────────────────────────

	async keysUpload(body: {
		device_keys?: Record<string, unknown>;
		one_time_keys?: Record<string, unknown>;
	}): Promise<{ one_time_key_counts: Record<string, number> }> {
		return this.request('POST', '/_matrix/client/v3/keys/upload', {
			headers: this.authHeaders(),
			body
		});
	}

	async keysQuery(
		userIds: string[]
	): Promise<{ device_keys: Record<string, Record<string, unknown>> }> {
		const device_keys: Record<string, string[]> = {};
		for (const uid of userIds) device_keys[uid] = [];
		return this.request('POST', '/_matrix/client/v3/keys/query', {
			headers: this.authHeaders(),
			body: { device_keys }
		});
	}

	async keysClaim(
		oneTimeKeys: Record<string, Record<string, string>>
	): Promise<{ one_time_keys: Record<string, Record<string, unknown>> }> {
		return this.request('POST', '/_matrix/client/v3/keys/claim', {
			headers: this.authHeaders(),
			body: { one_time_keys: oneTimeKeys }
		});
	}

	async sendToDevice(
		eventType: string,
		messages: Record<string, Record<string, unknown>>
	): Promise<void> {
		const txnId = crypto.randomUUID().replace(/-/g, '');
		await this.request(
			'PUT',
			`/_matrix/client/v3/sendToDevice/${encodeURIComponent(eventType)}/${txnId}`,
			{
				headers: this.authHeaders(),
				body: { messages }
			}
		);
	}
}

export const api = new AgoraApi();
