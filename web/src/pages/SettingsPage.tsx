import { type FormEvent, useCallback, useEffect, useState } from "react";
import {
  ApiError,
  api,
  type Language,
  type Profile,
  type SystemPrefs,
} from "../api/client";
import { errorText, type MessageKey, setLanguage, t } from "../i18n";
import { AccountPage } from "./AccountPage";
import { SecuritySection } from "./SecuritySection";
import { TasksSection } from "./TasksSection";

const REPO = "https://github.com/Skadli/ownmind";

const SECTIONS = [
  "profile",
  "tasks",
  "account",
  "system",
  "security",
  "feedback",
  "about",
] as const;
type Section = (typeof SECTIONS)[number];

const sectionTitle: Record<Section, MessageKey> = {
  profile: "settings.profile",
  tasks: "settings.tasks",
  account: "account.title",
  system: "settings.system",
  security: "settings.security",
  feedback: "settings.feedback",
  about: "settings.about",
};

const input =
  "w-full rounded-lg border border-neutral-300 px-3 py-2 focus:border-neutral-900 focus:outline-none";
const card = "space-y-4 rounded-2xl bg-white p-6 shadow-sm";

type Props = {
  onBack: () => void;
  onLoggedOut: () => void;
  /** 语言变了，整个界面要重新渲染。 */
  onLanguageChange: () => void;
};

/** 仿 Hark 的设置页：左侧栏 + 右侧内容（票 13）。 */
export function SettingsPage({ onBack, onLoggedOut, onLanguageChange }: Props) {
  const [section, setSection] = useState<Section>("profile");

  const logOut = async () => {
    await api.logOut().catch(() => {});
    onLoggedOut();
  };

  return (
    <main className="mx-auto flex max-w-5xl flex-col gap-6 px-4 py-8 md:flex-row">
      <nav className="flex shrink-0 flex-row flex-wrap gap-1 md:w-48 md:flex-col">
        <button
          type="button"
          onClick={onBack}
          className="mb-2 px-3 py-2 text-left text-sm text-neutral-500 hover:text-neutral-900"
        >
          ← {t("nav.back")}
        </button>
        {SECTIONS.map((s) => (
          <button
            type="button"
            key={s}
            onClick={() => setSection(s)}
            className={`rounded-lg px-3 py-2 text-left ${
              section === s
                ? "bg-neutral-900 text-white"
                : "text-neutral-700 hover:bg-neutral-100"
            }`}
          >
            {t(sectionTitle[s])}
          </button>
        ))}
        <button
          type="button"
          onClick={logOut}
          className="rounded-lg px-3 py-2 text-left text-red-600 hover:bg-red-50 md:mt-4"
        >
          {t("logout")}
        </button>
      </nav>
      <div className="min-w-0 flex-1 space-y-6">
        {section === "profile" && <ProfileSection onLoggedOut={onLoggedOut} />}
        {section === "tasks" && <TasksSection onLoggedOut={onLoggedOut} />}
        {section === "account" && (
          <AccountPage onUnauthenticated={onLoggedOut} />
        )}
        {section === "system" && (
          <SystemSection
            onLoggedOut={onLoggedOut}
            onLanguageChange={onLanguageChange}
          />
        )}
        {section === "security" && (
          <SecuritySection onUnauthenticated={onLoggedOut} />
        )}
        {section === "feedback" && (
          <section className={card}>
            <h2 className="text-lg font-semibold">{t("settings.feedback")}</h2>
            <p className="text-neutral-500">{t("settings.feedbackHint")}</p>
            <a
              href={`${REPO}/issues`}
              target="_blank"
              rel="noreferrer"
              className="inline-block rounded-lg bg-neutral-900 px-4 py-2 text-white"
            >
              {t("settings.feedbackOpen")}
            </a>
          </section>
        )}
        {section === "about" && (
          <section className={card}>
            <h2 className="text-lg font-semibold">{t("settings.about")}</h2>
            <p>{t("about.text")}</p>
            <a
              href={REPO}
              target="_blank"
              rel="noreferrer"
              className="underline"
            >
              {t("about.source")}
            </a>
          </section>
        )}
      </div>
    </main>
  );
}

/** 读写一份设置，处理登录过期与错误提示。 */
function useSetting<T>(
  load: () => Promise<T>,
  save: (v: T) => Promise<T>,
  onLoggedOut: () => void,
) {
  const [value, setValue] = useState<T | null>(null);
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

  // biome-ignore lint/correctness/useExhaustiveDependencies: 只在打开时加载一次
  useEffect(() => {
    load().then(setValue, handle);
  }, []);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (value === null) return;
    setError(null);
    setSaved(false);
    try {
      setValue(await save(value));
      setSaved(true);
      return true;
    } catch (err) {
      handle(err);
      return false;
    }
  };

  return { value, setValue, error, saved, submit };
}

function SaveRow({ error, saved }: { error: string | null; saved: boolean }) {
  return (
    <div className="flex items-center gap-3">
      <button
        type="submit"
        className="rounded-lg bg-neutral-900 px-4 py-2 text-white"
      >
        {t("common.save")}
      </button>
      {saved && (
        <span className="text-sm text-green-700">{t("common.saved")}</span>
      )}
      {error && <span className="text-sm text-red-600">{error}</span>}
    </div>
  );
}

function ProfileSection({ onLoggedOut }: { onLoggedOut: () => void }) {
  const s = useSetting<Profile>(api.getProfile, api.setProfile, onLoggedOut);
  if (!s.value) return null;
  const p = s.value;
  const set = (patch: Partial<Profile>) => s.setValue({ ...p, ...patch });
  return (
    <form onSubmit={s.submit} className={card}>
      <h2 className="text-lg font-semibold">{t("settings.profile")}</h2>
      <p className="text-sm text-neutral-500">{t("settings.profileHint")}</p>
      <label className="block space-y-1">
        <span className="text-sm font-medium">{t("profile.name")}</span>
        <input
          value={p.name}
          onChange={(e) => set({ name: e.target.value })}
          className={input}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-sm font-medium">
          {t("profile.preferredName")}
        </span>
        <input
          value={p.preferredName}
          onChange={(e) => set({ preferredName: e.target.value })}
          className={input}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-sm font-medium">{t("profile.bio")}</span>
        <textarea
          value={p.bio}
          rows={4}
          onChange={(e) => set({ bio: e.target.value })}
          className={input}
        />
      </label>
      <SaveRow error={s.error} saved={s.saved} />
    </form>
  );
}

/** 浏览器支持的全部 IANA 时区；不支持时只给当前时区。 */
function timezones(current: string): string[] {
  const intl = Intl as { supportedValuesOf?: (k: string) => string[] };
  const all = intl.supportedValuesOf?.("timeZone") ?? [];
  return all.includes(current) ? all : [current, ...all];
}

function SystemSection({
  onLoggedOut,
  onLanguageChange,
}: {
  onLoggedOut: () => void;
  onLanguageChange: () => void;
}) {
  const s = useSetting<SystemPrefs>(
    async () => {
      const v = await api.getSystem();
      // 还没设置过时区：默认用浏览器的。
      return v.timezone === "UTC"
        ? { ...v, timezone: Intl.DateTimeFormat().resolvedOptions().timeZone }
        : v;
    },
    api.setSystem,
    onLoggedOut,
  );
  if (!s.value) return null;
  const v = s.value;
  const submit = async (e: FormEvent) => {
    if (await s.submit(e)) {
      setLanguage(v.language);
      onLanguageChange();
    }
  };
  const languages: [Language, string][] = [
    ["auto", t("system.languageAuto")],
    ["zh", "中文"],
    ["en", "English"],
  ];
  return (
    <form onSubmit={submit} className={card}>
      <h2 className="text-lg font-semibold">{t("settings.system")}</h2>
      <label className="block space-y-1">
        <span className="text-sm font-medium">{t("system.language")}</span>
        <select
          value={v.language}
          onChange={(e) =>
            s.setValue({ ...v, language: e.target.value as Language })
          }
          className={input}
        >
          {languages.map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <label className="block space-y-1">
        <span className="text-sm font-medium">{t("system.timezone")}</span>
        <select
          value={v.timezone}
          onChange={(e) => s.setValue({ ...v, timezone: e.target.value })}
          className={input}
        >
          {timezones(v.timezone).map((tz) => (
            <option key={tz} value={tz}>
              {tz}
            </option>
          ))}
        </select>
      </label>
      <SaveRow error={s.error} saved={s.saved} />
    </form>
  );
}
