import { type FormEvent, useState } from "react";
import { ApiError } from "../api/client";
import { errorText, t } from "../i18n";

type Props = {
  mode: "setup" | "login";
  onSubmit: (password: string) => Promise<void>;
};

/** 首次设置密码与登录共用的页面。 */
export function PasswordPage({ mode, onSubmit }: Props) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await onSubmit(password);
    } catch (err) {
      setError(errorText(err instanceof ApiError ? err.code : "internal"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-neutral-50 px-4">
      <form
        onSubmit={submit}
        className="w-full max-w-sm space-y-4 rounded-2xl bg-white p-8 shadow-sm"
      >
        <h1 className="text-2xl font-semibold">
          {mode === "setup" ? t("setup.title") : t("login.title")}
        </h1>
        {mode === "setup" && (
          <p className="text-sm text-neutral-500">{t("setup.hint")}</p>
        )}
        <label className="block space-y-1">
          <span className="text-sm font-medium">{t("password.label")}</span>
          <input
            type="password"
            autoComplete={
              mode === "setup" ? "new-password" : "current-password"
            }
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="w-full rounded-lg border border-neutral-300 px-3 py-2 outline-none focus:border-neutral-900"
          />
        </label>
        {error && <p className="text-sm text-red-600">{error}</p>}
        <button
          type="submit"
          disabled={busy || password.length === 0}
          className="w-full rounded-lg bg-neutral-900 py-2 text-white disabled:opacity-40"
        >
          {mode === "setup" ? t("setup.submit") : t("login.submit")}
        </button>
      </form>
    </main>
  );
}
