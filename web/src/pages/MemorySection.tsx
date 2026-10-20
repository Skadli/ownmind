import { type FormEvent, useCallback, useEffect, useState } from "react";
import { ApiError, api, type MemoryView, type SkillInfo } from "../api/client";
import { errorText, t } from "../i18n";

const input =
  "w-full rounded-lg border border-neutral-300 px-3 py-2 focus:border-neutral-900 focus:outline-none";
const card = "space-y-4 rounded-2xl bg-white p-6 shadow-sm";
const btn = "rounded-lg border border-neutral-300 px-3 py-1 text-sm";

export function MemorySection({ onLoggedOut }: { onLoggedOut: () => void }) {
  const [mem, setMem] = useState<MemoryView | null>(null);
  const [skills, setSkills] = useState<SkillInfo[] | null>(null);
  const [core, setCore] = useState("");
  const [entry, setEntry] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const handle = useCallback(
    (err: unknown) => {
      const code = err instanceof ApiError ? err.code : "internal";
      if (code === "unauthenticated") return onLoggedOut();
      setError(errorText(code));
    },
    [onLoggedOut],
  );
  const load = useCallback(async () => {
    try {
      const m = await api.getMemory();
      setMem(m);
      setCore(m.core);
      setSkills(await api.listSkills());
    } catch (err) {
      handle(err);
    }
  }, [handle]);
  useEffect(() => {
    load();
  }, [load]);

  const run = async (job: () => Promise<unknown>, reloadAll = false) => {
    setError(null);
    setSaved(false);
    try {
      await job();
      if (reloadAll) await load();
      else setMem(await api.getMemory());
      return true;
    } catch (err) {
      handle(err);
      // 冲突时重新载入最新核心记忆
      if (err instanceof ApiError && err.code === "memory_conflict") {
        await load();
        setError(errorText(err.code));
      }
      return false;
    }
  };

  const saveCore = async (e: FormEvent) => {
    e.preventDefault();
    if (!mem) return;
    if (await run(() => api.setMemoryCore(mem.core, core), true))
      setSaved(true);
  };
  const addEntry = async (e: FormEvent) => {
    e.preventDefault();
    if (!entry.trim()) return;
    if (await run(() => api.addMemoryEntry(entry.trim()))) setEntry("");
  };

  return (
    <div className="space-y-6">
      <section className={card}>
        <h2 className="text-lg font-semibold">{t("settings.memory")}</h2>
        <p className="text-sm text-neutral-500">{t("memory.hint")}</p>
        {error && <p className="text-sm text-red-600">{error}</p>}
        <form onSubmit={saveCore} className="space-y-3">
          <label className="block space-y-1">
            <span className="text-sm font-medium">{t("memory.core")}</span>
            <textarea
              className={input}
              rows={6}
              value={core}
              onChange={(e) => setCore(e.target.value)}
            />
          </label>
          <div className="flex items-center gap-3">
            <button type="submit" className={btn} disabled={!mem}>
              {t("memory.save")}
            </button>
            {saved && (
              <span className="text-sm text-green-700">
                {t("common.saved")}
              </span>
            )}
          </div>
        </form>
      </section>
      <section className={card}>
        <h3 className="font-semibold">{t("memory.entries")}</h3>
        <form onSubmit={addEntry} className="flex gap-2">
          <input
            className={input}
            placeholder={t("memory.addPlaceholder")}
            value={entry}
            onChange={(e) => setEntry(e.target.value)}
          />
          <button type="submit" className={btn}>
            {t("memory.add")}
          </button>
        </form>
        {mem?.entries.length === 0 && (
          <p className="text-neutral-500">{t("memory.entriesEmpty")}</p>
        )}
        <ul className="space-y-2">
          {mem?.entries.map((m) => (
            <li
              key={m.id}
              className="flex items-start justify-between gap-3 rounded-xl border border-neutral-200 p-3"
            >
              <div className="min-w-0">
                <p className="whitespace-pre-wrap text-sm">{m.content}</p>
                <p className="text-xs text-neutral-500">
                  {new Date(m.createdAt).toLocaleString()}
                </p>
              </div>
              <button
                type="button"
                className={`${btn} text-red-600`}
                onClick={() => run(() => api.deleteMemoryEntry(m.id))}
              >
                {t("memory.delete")}
              </button>
            </li>
          ))}
        </ul>
      </section>
      <section className={card}>
        <h3 className="font-semibold">{t("skills.title")}</h3>
        {skills?.length === 0 && (
          <p className="text-neutral-500">{t("skills.empty")}</p>
        )}
        <ul className="space-y-2">
          {skills?.map((s) => (
            <li
              key={s.name}
              className="flex items-start justify-between gap-3 rounded-xl border border-neutral-200 p-3"
            >
              <div className="min-w-0">
                <p className="text-sm font-medium">
                  {s.name}{" "}
                  <span className="text-xs font-normal text-neutral-500">
                    {s.source === "builtin"
                      ? t("skills.builtin")
                      : t("skills.installed")}
                  </span>
                </p>
                <p className="text-sm text-neutral-600">{s.description}</p>
              </div>
              {s.source === "installed" && (
                <button
                  type="button"
                  className={`${btn} text-red-600`}
                  onClick={() => run(() => api.removeSkill(s.name), true)}
                >
                  {t("skills.remove")}
                </button>
              )}
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
