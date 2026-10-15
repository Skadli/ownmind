import type { Conversation } from "./generated/conversation";
import type {
  Credential,
  CredentialInput,
  DefaultModel,
  ProviderKind,
} from "./generated/settings";

export type {
  Conversation,
  Credential,
  CredentialInput,
  DefaultModel,
  ProviderKind,
};

/** 接口错误，code 对应后端的 `{"error": code}`。 */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
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
    throw new ApiError(res.status, data.error ?? "internal");
  }
  return res.status === 204 ? (undefined as T) : res.json();
}

export const api = {
  status: () =>
    request<{ isSetUp: boolean; isLoggedIn: boolean }>("GET", "/auth/status"),
  setUp: (password: string) =>
    request<void>("POST", "/auth/setup", { password }),
  logIn: (password: string) =>
    request<void>("POST", "/auth/login", { password }),
  logOut: () => request<void>("POST", "/auth/logout"),
  listConversations: () => request<Conversation[]>("GET", "/conversations"),
  createConversation: () => request<Conversation>("POST", "/conversations", {}),
  renameConversation: (id: string, title: string) =>
    request<Conversation>("PATCH", `/conversations/${id}`, { title }),
  deleteConversation: (id: string) =>
    request<void>("DELETE", `/conversations/${id}`),
  listCredentials: () => request<Credential[]>("GET", "/settings/credentials"),
  createCredential: (input: CredentialInput) =>
    request<Credential>("POST", "/settings/credentials", input),
  updateCredential: (id: string, input: CredentialInput) =>
    request<Credential>("PATCH", `/settings/credentials/${id}`, input),
  deleteCredential: (id: string) =>
    request<void>("DELETE", `/settings/credentials/${id}`),
  getDefaultModel: () =>
    request<DefaultModel | null>("GET", "/settings/default-model"),
  setDefaultModel: (model: DefaultModel) =>
    request<void>("PUT", "/settings/default-model", model),
};
