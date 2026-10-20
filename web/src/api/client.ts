import type { Fire, Task, TaskDraft } from "../pages/schedule";
import type { ChatEvent } from "./generated/chat";
import type { Conversation, Message } from "./generated/conversation";
import type {
  Credential,
  CredentialInput,
  DefaultModel,
  Language,
  Profile,
  ProviderKind,
  SearchConfig,
  SearchInput,
  SearchProvider,
  SystemPrefs,
} from "./generated/settings";
import { SseParser } from "./sse";

export type {
  ChatEvent,
  Conversation,
  Credential,
  CredentialInput,
  DefaultModel,
  Language,
  Message,
  Profile,
  ProviderKind,
  SearchConfig,
  SearchInput,
  SearchProvider,
  SystemPrefs,
};

/** 接口错误，code 对应后端的 `{"error": code}`。 */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    /** 登录被锁定时的剩余秒数。 */
    readonly retryAfter?: number,
  ) {
    super(code);
  }
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    headers: body === undefined ? {} : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({ error: "internal" }));
    throw new ApiError(res.status, data.error ?? "internal", data.retryAfter);
  }
  return res.status === 204 ? (undefined as T) : res.json();
}

export const api = {
  status: () =>
    request<{ isSetUp: boolean; isLoggedIn: boolean }>("GET", "/auth/status"),
  setUp: (password: string) =>
    request<void>("POST", "/auth/setup", { password }),
  logIn: (password: string, code?: string) =>
    request<void>("POST", "/auth/login", { password, code }),
  totpStatus: () => request<{ enabled: boolean }>("GET", "/auth/totp"),
  totpSetup: () =>
    request<{ secret: string; uri: string }>("POST", "/auth/totp/setup"),
  totpConfirm: (code: string) =>
    request<void>("POST", "/auth/totp/confirm", { code }),
  totpDisable: (password: string) =>
    request<void>("POST", "/auth/totp/disable", { password }),
  logOut: () => request<void>("POST", "/auth/logout"),
  listConversations: () => request<Conversation[]>("GET", "/conversations"),
  createConversation: () => request<Conversation>("POST", "/conversations", {}),
  renameConversation: (id: string, title: string) =>
    request<Conversation>("PATCH", `/conversations/${id}`, { title }),
  deleteConversation: (id: string) =>
    request<void>("DELETE", `/conversations/${id}`),
  listMessages: (id: string) =>
    request<Message[]>("GET", `/conversations/${id}/messages`),
  runStatus: (id: string) =>
    request<{ running: boolean }>("GET", `/conversations/${id}/run`),
  stopRun: (id: string) => request<void>("DELETE", `/conversations/${id}/run`),
  sendMessage,
  listCredentials: () => request<Credential[]>("GET", "/settings/credentials"),
  createCredential: (input: CredentialInput) =>
    request<Credential>("POST", "/settings/credentials", input),
  updateCredential: (id: string, input: CredentialInput) =>
    request<Credential>("PATCH", `/settings/credentials/${id}`, input),
  deleteCredential: (id: string) =>
    request<void>("DELETE", `/settings/credentials/${id}`),
  getSearch: () => request<SearchConfig | null>("GET", "/settings/search"),
  setSearch: (input: SearchInput) =>
    request<SearchConfig>("PUT", "/settings/search", input),
  clearSearch: () => request<void>("DELETE", "/settings/search"),
  getProfile: () => request<Profile>("GET", "/settings/profile"),
  setProfile: (p: Profile) => request<Profile>("PUT", "/settings/profile", p),
  getSystem: () => request<SystemPrefs>("GET", "/settings/system"),
  setSystem: (p: SystemPrefs) =>
    request<SystemPrefs>("PUT", "/settings/system", p),
  getDefaultModel: () =>
    request<DefaultModel | null>("GET", "/settings/default-model"),
  listTasks: () => request<Task[]>("GET", "/schedule/tasks"),
  createTask: (t: TaskDraft) => request<Task>("POST", "/schedule/tasks", t),
  updateTask: (id: string, t: Partial<TaskDraft> & { enabled?: boolean }) =>
    request<Task>("PATCH", `/schedule/tasks/${id}`, t),
  deleteTask: (id: string) => request<void>("DELETE", `/schedule/tasks/${id}`),
  listFires: (id: string) =>
    request<Fire[]>("GET", `/schedule/tasks/${id}/fires`),
  setDefaultModel: (model: DefaultModel) =>
    request<void>("PUT", "/settings/default-model", model),
};

/** 发消息并逐个回调 SSE 事件，流结束时返回。 */
async function sendMessage(
  id: string,
  text: string,
  onEvent: (e: ChatEvent) => void,
): Promise<void> {
  const res = await fetch(`/api/conversations/${id}/messages`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
  });
  if (!res.ok || !res.body) {
    const data = await res.json().catch(() => ({ error: "internal" }));
    throw new ApiError(res.status, data.error ?? "internal");
  }
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  const parser = new SseParser();
  for (;;) {
    const { value, done } = await reader.read();
    if (done) return;
    for (const data of parser.push(value)) onEvent(JSON.parse(data));
  }
}
