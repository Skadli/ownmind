import { type FormEvent, useCallback, useEffect, useState } from "react";
import { ApiError, api } from "../api/client";
import { errorText, t } from "../i18n";
import {
  type Fire,
  type IntervalUnit,
  intervalSecs,
  localToMs,
  type Schedule,
  type Task,
} from "./schedule";

const input =
  "w-full rounded-lg border border-neutral-300 px-3 py-2 focus:border-neutral-900 focus:outline-none";
const card = "space-y-4 rounded-2xl bg-white p-6 shadow-sm";
const btn = "rounded-lg border border-neutral-300 px-3 py-1 text-sm";

function fmt(ms: number | null): string {
  return ms === null ? t("tasks.never") : new Date(ms).toLocaleString();
}

function describe(s: Schedule): string {
  if (s.kind === "once") return `${t("tasks.onceText")} · ${fmt(s.runAt)}`;
  if (s.everySecs % 60 !== 0) return `${t("tasks.everyText")} ${s.everySecs} s`;
  const mins = s.everySecs / 60;
  if (mins % 1440 === 0)
    return `${t("tasks.everyText")} ${mins / 1440} ${t("tasks.days")}`;
  if (mins % 60 === 0)
    return `${t("tasks.everyText")} ${mins / 60} ${t("tasks.hours")}`;
  return `${t("tasks.everyText")} ${mins} ${t("tasks.minutes")}`;
}

export function TasksSection({ onLoggedOut }: { onLoggedOut: () => void }) {
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fires, setFires] = useState<{ id: string; list: Fire[] } | null>(null);

  const handle = useCallback(
    (err: unknown) => {
      const code = err instanceof ApiError ? err.code : "internal";
      if (code === "unauthenticated") return onLoggedOut();
      setError(errorText(code));
    },
    [onLoggedOut],
  );
  const reload = useCallback(
    () => api.listTasks().then(setTasks, handle),
    [handle],
  );
  useEffect(() => {
    reload();
  }, [reload]);

  const run = async (job: () => Promise<unknown>): Promise<boolean> => {
    setError(null);
    try {
      await job();
      await reload();
      return true;
    } catch (err) {
      handle(err);
      return false;
    }
  };
  const showFires = async (id: string) => {
    if (fires?.id === id) return setFires(null);
    try {
      setFires({ id, list: await api.listFires(id) });
    } catch (err) {
      handle(err);
    }
  };

  return (
    <div className="space-y-6">
      <section className={card}>
        <h2 className="text-lg font-semibold">{t("settings.tasks")}</h2>
        <p className="text-sm text-neutral-500">{t("tasks.hint")}</p>
        {error && <p className="text-sm text-red-600">{error}</p>}
        {tasks?.length === 0 && (
          <p className="text-neutral-500">{t("tasks.empty")}</p>
        )}
        <ul className="space-y-3">
          {tasks?.map((task) => (
            <li
              key={task.id}
              className="space-y-2 rounded-xl border border-neutral-200 p-4"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-medium">{task.name}</span>
                <span className="text-sm text-neutral-500">
                  {describe(task.schedule)}
                </span>
              </div>
              <p className="whitespace-pre-wrap text-sm text-neutral-700">
                {task.prompt}
              </p>
              <p className="text-xs text-neutral-500">
                {t("tasks.next")}：
                {task.nextRunAt === null
                  ? t("tasks.done")
                  : fmt(task.nextRunAt)}
                {" · "}
                {t("tasks.last")}：{fmt(task.lastRunAt)}
              </p>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  className={btn}
                  onClick={() =>
                    run(() =>
                      api.updateTask(task.id, { enabled: !task.enabled }),
                    )
                  }
                >
                  {task.enabled ? t("tasks.disable") : t("tasks.enable")}
                </button>
                <button
                  type="button"
                  className={btn}
                  onClick={() => showFires(task.id)}
                >
                  {t("tasks.fires")}
                </button>
                <button
                  type="button"
                  className={`${btn} text-red-600`}
                  onClick={() =>
                    window.confirm(t("tasks.confirmDelete")) &&
                    run(() => api.deleteTask(task.id))
                  }
                >
                  {t("tasks.delete")}
                </button>
              </div>
              {fires?.id === task.id && (
                <ul className="space-y-1 text-xs text-neutral-600">
                  {fires.list.length === 0 && <li>{t("tasks.noFires")}</li>}
                  {fires.list.map((f) => (
                    <li key={f.id}>
                      {fmt(f.firedAt)} · {f.status}
                      {f.error ? ` · ${f.error}` : ""}
                    </li>
                  ))}
                </ul>
              )}
            </li>
          ))}
        </ul>
      </section>
      <NewTask
        onCreate={(draft) => run(() => api.createTask(draft))}
        onInvalid={setError}
      />
    </div>
  );
}

function NewTask({
  onCreate,
  onInvalid,
}: {
  onCreate: (d: {
    name: string;
    prompt: string;
    schedule: Schedule;
  }) => Promise<void>;
  onInvalid: (msg: string) => void;
}) {
  const [name, setName] = useState("");
  const [prompt, setPrompt] = useState("");
  const [kind, setKind] = useState<"interval" | "once">("interval");
  const [amount, setAmount] = useState("1");
  const [unit, setUnit] = useState<IntervalUnit>("hours");
  const [runAt, setRunAt] = useState("");

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    let schedule: Schedule | null = null;
    if (kind === "interval") {
      const secs = intervalSecs(Number(amount), unit);
      if (secs !== null) schedule = { kind, everySecs: secs };
    } else {
      const ms = localToMs(runAt);
      if (ms !== null) schedule = { kind, runAt: ms };
    }
    if (!name.trim() || !prompt.trim() || !schedule) {
      return onInvalid(t("tasks.badInput"));
    }
    if (
      await onCreate({ name: name.trim(), prompt: prompt.trim(), schedule })
    ) {
      setName("");
      setPrompt("");
    }
  };

  return (
    <form onSubmit={submit} className={card}>
      <h3 className="font-semibold">{t("tasks.new")}</h3>
      <label className="block space-y-1">
        <span className="text-sm font-medium">{t("tasks.name")}</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          className={input}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-sm font-medium">{t("tasks.prompt")}</span>
        <textarea
          value={prompt}
          rows={3}
          onChange={(e) => setPrompt(e.target.value)}
          className={input}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-sm font-medium">{t("tasks.kind")}</span>
        <select
          value={kind}
          onChange={(e) => setKind(e.target.value as "interval" | "once")}
          className={input}
        >
          <option value="interval">{t("tasks.interval")}</option>
          <option value="once">{t("tasks.once")}</option>
        </select>
      </label>
      {kind === "interval" ? (
        <div className="space-y-1">
          <span className="text-sm font-medium">{t("tasks.every")}</span>
          <div className="flex gap-2">
            <input
              type="number"
              min={1}
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              className={input}
            />
            <select
              value={unit}
              onChange={(e) => setUnit(e.target.value as IntervalUnit)}
              className={input}
            >
              <option value="minutes">{t("tasks.minutes")}</option>
              <option value="hours">{t("tasks.hours")}</option>
              <option value="days">{t("tasks.days")}</option>
            </select>
          </div>
        </div>
      ) : (
        <label className="block space-y-1">
          <span className="text-sm font-medium">{t("tasks.runAt")}</span>
          <input
            type="datetime-local"
            value={runAt}
            onChange={(e) => setRunAt(e.target.value)}
            className={input}
          />
        </label>
      )}
      <button
        type="submit"
        className="rounded-lg bg-neutral-900 px-4 py-2 text-white"
      >
        {t("common.save")}
      </button>
    </form>
  );
}
