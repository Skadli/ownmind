import { type FormEvent, useCallback, useEffect, useState } from "react";
import {
  ApiError,
  api,
  type Credential,
  type CredentialInput,
  type DefaultModel,
  type ProviderKind,
} from "../api/client";
import { errorText, type MessageKey, t } from "../i18n";

const KINDS: ProviderKind[] = [
  "openai-compatible",
  "anthropic-compatible",
  "openai",
  "anthropic",
  "google",
  "deepseek",
  "openrouter",
];

const kindName = (k: ProviderKind) => t(`kind.${k}` as MessageKey);
const needsUrl = (k: ProviderKind) => k.endsWith("-compatible");

type Draft = CredentialInput & { id?: string };

const input =
  "w-full rounded-lg border border-neutral-300 px-3 py-2 focus:border-neutral-900 focus:outline-none";

type Props = { onUnauthenticated: () => void };

export function AccountPage({ onUnauthenticated }: Props) {
  const [items, setItems] = useState<Credential[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [model, setModel] = useState<DefaultModel>({
    credentialId: "",
    modelId: "",
  });
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const run = useCallback(
    async (action: () => Promise<void>) => {
      setError(null);
      setSaved(false);
      try {
        await action();
      } catch (err) {
        if (err instanceof ApiError && err.code === "unauthenticated")
          return onUnauthenticated();
        setError(errorText(err instanceof ApiError ? err.code : "internal"));
      }
    },
    [onUnauthenticated],
  );

  const reload = useCallback(
    () =>
      run(async () => {
        const [list, current] = await Promise.all([
          api.listCredentials(),
          api.getDefaultModel(),
        ]);
        setItems(list);
        setModel(current ?? { credentialId: list[0]?.id ?? "", modelId: "" });
      }),
    [run],
  );

  useEffect(() => {
    reload();
  }, [reload]);

  const saveDraft = (e: FormEvent) => {
    e.preventDefault();
    if (!draft) return;
    const { id, ...body } = draft;
    run(async () => {
      if (id) await api.updateCredential(id, body);
      else await api.createCredential(body);
      setDraft(null);
      await reload();
    });
  };

  const remove = (id: string) => {
    if (!window.confirm(t("account.deleteConfirm"))) return;
    run(async () => {
      await api.deleteCredential(id);
      await reload();
    });
  };

  const saveModel = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      await api.setDefaultModel(model);
      setSaved(true);
    });
  };

  return (
    <section className="space-y-8">
      <div>
        <h2 className="text-xl font-semibold">{t("account.title")}</h2>
        <p className="mt-1 text-sm text-neutral-500">{t("account.hint")}</p>
      </div>
      {error && <p className="text-sm text-red-600">{error}</p>}

      <div className="space-y-3">
        {items.length === 0 && !draft && (
          <p className="text-neutral-500">{t("account.empty")}</p>
        )}
        {items.length > 0 && (
          <ul className="divide-y divide-neutral-200 rounded-2xl bg-white shadow-sm">
            {items.map((c) => (
              <li key={c.id} className="flex items-center gap-3 px-4 py-3">
                <div className="flex-1 min-w-0">
                  <p className="truncate font-medium">{c.label}</p>
                  <p className="truncate text-sm text-neutral-500">
                    {kindName(c.kind)}
                    {c.baseUrl ? ` · ${c.baseUrl}` : ""} · {c.keyHint}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() =>
                    setDraft({
                      id: c.id,
                      kind: c.kind,
                      label: c.label,
                      baseUrl: c.baseUrl ?? "",
                      apiKey: "",
                    })
                  }
                  className="text-sm text-neutral-500 hover:text-neutral-900"
                >
                  {t("account.edit")}
                </button>
                <button
                  type="button"
                  onClick={() => remove(c.id)}
                  className="text-sm text-red-600"
                >
                  {t("account.delete")}
                </button>
              </li>
            ))}
          </ul>
        )}

        {draft ? (
          <form
            onSubmit={saveDraft}
            className="space-y-3 rounded-2xl bg-white p-4 shadow-sm"
          >
            <label className="block space-y-1">
              <span className="text-sm">{t("account.kind")}</span>
              <select
                value={draft.kind}
                onChange={(e) => {
                  const kind = e.target.value as ProviderKind;
                  setDraft({
                    ...draft,
                    kind,
                    label: draft.label || kindName(kind),
                  });
                }}
                className={input}
              >
                {KINDS.map((k) => (
                  <option key={k} value={k}>
                    {kindName(k)}
                  </option>
                ))}
              </select>
            </label>
            <label className="block space-y-1">
              <span className="text-sm">{t("account.label")}</span>
              <input
                value={draft.label}
                onChange={(e) => setDraft({ ...draft, label: e.target.value })}
                className={input}
              />
            </label>
            <label className="block space-y-1">
              <span className="text-sm">{t("account.baseUrl")}</span>
              <input
                value={draft.baseUrl ?? ""}
                onChange={(e) =>
                  setDraft({ ...draft, baseUrl: e.target.value })
                }
                placeholder={needsUrl(draft.kind) ? "https://…/v1" : ""}
                className={input}
              />
              <span className="text-xs text-neutral-500">
                {t("account.baseUrlHint")}
              </span>
            </label>
            <label className="block space-y-1">
              <span className="text-sm">{t("account.apiKey")}</span>
              <input
                type="password"
                autoComplete="off"
                value={draft.apiKey ?? ""}
                onChange={(e) => setDraft({ ...draft, apiKey: e.target.value })}
                placeholder={draft.id ? t("account.apiKeyKeep") : ""}
                className={input}
              />
            </label>
            <div className="flex gap-2">
              <button
                type="submit"
                className="rounded-lg bg-neutral-900 px-4 py-2 text-white"
              >
                {t("account.save")}
              </button>
              <button
                type="button"
                onClick={() => setDraft(null)}
                className="rounded-lg px-4 py-2 text-neutral-600 hover:bg-neutral-100"
              >
                {t("account.cancel")}
              </button>
            </div>
          </form>
        ) : (
          <button
            type="button"
            onClick={() =>
              setDraft({
                kind: "openai-compatible",
                label: kindName("openai-compatible"),
                baseUrl: "",
                apiKey: "",
              })
            }
            className="rounded-lg border border-neutral-300 px-4 py-2 hover:bg-neutral-100"
          >
            {t("account.add")}
          </button>
        )}
      </div>

      {items.length > 0 && (
        <form
          onSubmit={saveModel}
          className="space-y-3 rounded-2xl bg-white p-4 shadow-sm"
        >
          <div>
            <h3 className="font-medium">{t("account.defaultModel")}</h3>
            <p className="text-sm text-neutral-500">
              {t("account.defaultModelHint")}
            </p>
          </div>
          <div className="flex flex-col gap-2 sm:flex-row">
            <select
              value={model.credentialId}
              onChange={(e) =>
                setModel({ ...model, credentialId: e.target.value })
              }
              className={input}
            >
              {items.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label}
                </option>
              ))}
            </select>
            <input
              value={model.modelId}
              onChange={(e) => setModel({ ...model, modelId: e.target.value })}
              placeholder={t("account.modelId")}
              className={input}
            />
            <button
              type="submit"
              className="shrink-0 rounded-lg bg-neutral-900 px-4 py-2 text-white"
            >
              {t("account.save")}
            </button>
          </div>
          {saved && (
            <p className="text-sm text-green-700">{t("account.saved")}</p>
          )}
        </form>
      )}
    </section>
  );
}
