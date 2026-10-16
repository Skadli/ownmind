import { type FormEvent, useState } from "react";
import { ApiError } from "../api/client";
import { errorText, t } from "../i18n";

type Props = {
  mode: "setup" | "login";
  onSubmit: (password: string, code?: string) => Promise<void>;
};

/** 首次设置密码与登录共用的页面。 */
export function PasswordPage({ mode, onSubmit }: Props) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [code, setCode] = useState("");
  /** 服务端要求两步验证码后显示验证码输入框。 */
  const [needsCode, setNeedsCode] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await onSubmit(password, needsCode ? code : undefined);
    } catch (err) {
      const c = err instanceof ApiError ? err.code : "internal";
      if (c === "totp_required") {
        setNeedsCode(true);
        return;
      }
      const wait = err instanceof ApiError ? err.retryAfter : undefined;
      setError(
        wait
          ? `${errorText(c)} (${Math.ceil(wait / 60)} ${t("unit.minutes")})`
          : errorText(c),
      );
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
        {needsCode && (
          <label className="block space-y-1">
            <span className="text-sm font-medium">{t("totp.code")}</span>
            <input
              inputMode="numeric"
              autoComplete="one-time-code"
              // biome-ignore lint/a11y/noAutofocus: 密码通过后只剩这一个输入
              autoFocus
              value={code}
              onChange={(e) => setCode(e.target.value)}
              className="w-full rounded-lg border border-neutral-300 px-3 py-2 tracking-widest outline-none focus:border-neutral-900"
            />
          </label>
        )}
        {error && <p className="text-sm text-red-600">{error}</p>}
        <button
          type="submit"
          disabled={
            busy || password.length === 0 || (needsCode && !code.trim())
          }
          className="w-full rounded-lg bg-neutral-900 py-2 text-white disabled:opacity-40"
        >
          {mode === "setup" ? t("setup.submit") : t("login.submit")}
        </button>
      </form>
    </main>
  );
}
