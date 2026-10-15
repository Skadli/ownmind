import { type FormEvent, useCallback, useEffect, useState } from "react";
import { ApiError, api, type Conversation } from "../api/client";
import { errorText, t } from "../i18n";

type Props = { onLoggedOut: () => void; onOpenSettings: () => void };

export function ConversationsPage({ onLoggedOut, onOpenSettings }: Props) {
  const [items, setItems] = useState<Conversation[]>([]);
  const [editing, setEditing] = useState<{ id: string; title: string } | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    async (action: () => Promise<void>) => {
      setError(null);
      try {
        await action();
      } catch (err) {
        if (err instanceof ApiError && err.code === "unauthenticated")
          return onLoggedOut();
        setError(errorText(err instanceof ApiError ? err.code : "internal"));
      }
    },
    [onLoggedOut],
  );

  const reload = useCallback(
    () => run(async () => setItems(await api.listConversations())),
    [run],
  );

  useEffect(() => {
    reload();
  }, [reload]);

  const create = () =>
    run(async () => {
      await api.createConversation();
      await reload();
    });

  const saveTitle = (e: FormEvent) => {
    e.preventDefault();
    if (!editing) return;
    run(async () => {
      await api.renameConversation(editing.id, editing.title);
      setEditing(null);
      await reload();
    });
  };

  const remove = (id: string) => {
    if (!window.confirm(t("conversations.deleteConfirm"))) return;
    run(async () => {
      await api.deleteConversation(id);
      await reload();
    });
  };

  const logOut = () =>
    run(async () => {
      await api.logOut();
      onLoggedOut();
    });

  return (
    <main className="mx-auto max-w-2xl space-y-6 px-4 py-10">
      <header className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">{t("conversations.title")}</h1>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={create}
            className="rounded-lg bg-neutral-900 px-4 py-2 text-white"
          >
            {t("conversations.new")}
          </button>
          <button
            type="button"
            onClick={onOpenSettings}
            className="rounded-lg px-4 py-2 text-neutral-600 hover:bg-neutral-100"
          >
            {t("nav.settings")}
          </button>
          <button
            type="button"
            onClick={logOut}
            className="rounded-lg px-4 py-2 text-neutral-600 hover:bg-neutral-100"
          >
            {t("logout")}
          </button>
        </div>
      </header>
      {error && <p className="text-sm text-red-600">{error}</p>}
      {items.length === 0 ? (
        <p className="text-neutral-500">{t("conversations.empty")}</p>
      ) : (
        <ul className="divide-y divide-neutral-200 rounded-2xl bg-white shadow-sm">
          {items.map((c) => (
            <li key={c.id} className="flex items-center gap-3 px-4 py-3">
              {editing?.id === c.id ? (
                <form onSubmit={saveTitle} className="flex flex-1 gap-2">
                  <input
                    value={editing.title}
                    onChange={(e) =>
                      setEditing({ id: c.id, title: e.target.value })
                    }
                    className="flex-1 rounded-lg border border-neutral-300 px-2 py-1"
                  />
                  <button type="submit" className="text-sm font-medium">
                    {t("conversations.save")}
                  </button>
                  <button
                    type="button"
                    onClick={() => setEditing(null)}
                    className="text-sm text-neutral-500"
                  >
                    {t("conversations.cancel")}
                  </button>
                </form>
              ) : (
                <>
                  <span className="flex-1 truncate">{c.title}</span>
                  <button
                    type="button"
                    onClick={() => setEditing({ id: c.id, title: c.title })}
                    className="text-sm text-neutral-500 hover:text-neutral-900"
                  >
                    {t("conversations.rename")}
                  </button>
                  <button
                    type="button"
                    onClick={() => remove(c.id)}
                    className="text-sm text-red-600"
                  >
                    {t("conversations.delete")}
                  </button>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}
