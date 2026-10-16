import QRCode from "qrcode";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { ApiError, api } from "../api/client";
import { errorText, t } from "../i18n";

const input =
  "w-full rounded-lg border border-neutral-300 px-3 py-2 focus:border-neutral-900 focus:outline-none";

type Props = { onUnauthenticated: () => void };

type Mode =
  | { kind: "idle" }
  | { kind: "setup"; secret: string; qr: string }
  | { kind: "disable" };

/** 设置页的安全分区：两步验证（票 11）。 */
export function SecuritySection({ onUnauthenticated }: Props) {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [mode, setMode] = useState<Mode>({ kind: "idle" });
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    async (action: () => Promise<void>) => {
      setError(null);
      try {
        await action();
      } catch (err) {
        const code = err instanceof ApiError ? err.code : "internal";
        if (code === "unauthenticated") return onUnauthenticated();
        setError(errorText(code));
      }
    },
    [onUnauthenticated],
  );

  useEffect(() => {
    run(async () => setEnabled((await api.totpStatus()).enabled));
  }, [run]);

  const begin = () =>
    run(async () => {
      const s = await api.totpSetup();
      const qr = await QRCode.toDataURL(s.uri, { margin: 1, width: 200 });
      setValue("");
      setMode({ kind: "setup", secret: s.secret, qr });
    });

  const submit = (e: FormEvent) => {
    e.preventDefault();
    run(async () => {
      if (mode.kind === "setup") await api.totpConfirm(value);
      else await api.totpDisable(value);
      setEnabled(mode.kind === "setup");
      setMode({ kind: "idle" });
      setValue("");
    });
  };

  if (enabled === null) return null;

  return (
    <section className="space-y-4 rounded-2xl bg-white p-6 shadow-sm">
      <header className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">{t("totp.title")}</h2>
        <span
          className={`text-sm ${enabled ? "text-green-700" : "text-neutral-500"}`}
        >
          {enabled ? t("totp.on") : t("totp.off")}
        </span>
      </header>
      <p className="text-sm text-neutral-500">{t("totp.hint")}</p>

      {mode.kind === "idle" && (
        <button
          type="button"
          onClick={enabled ? () => setMode({ kind: "disable" }) : () => begin()}
          className="rounded-lg border border-neutral-300 px-4 py-2"
        >
          {enabled ? t("totp.disable") : t("totp.enable")}
        </button>
      )}

      {mode.kind !== "idle" && (
        <form onSubmit={submit} className="space-y-3">
          {mode.kind === "setup" && (
            <>
              <p className="text-sm">{t("totp.scan")}</p>
              <img src={mode.qr} alt="TOTP QR" width={200} height={200} />
              <code className="block break-all rounded-lg bg-neutral-100 px-3 py-2 text-sm">
                {mode.secret}
              </code>
            </>
          )}
          <label className="block space-y-1">
            <span className="text-sm font-medium">
              {mode.kind === "setup"
                ? t("totp.code")
                : t("totp.disablePassword")}
            </span>
            <input
              type={mode.kind === "setup" ? "text" : "password"}
              inputMode={mode.kind === "setup" ? "numeric" : undefined}
              autoComplete={
                mode.kind === "setup" ? "one-time-code" : "current-password"
              }
              value={value}
              onChange={(e) => setValue(e.target.value)}
              className={input}
            />
          </label>
          <div className="flex gap-2">
            <button
              type="submit"
              disabled={!value.trim()}
              className="rounded-lg bg-neutral-900 px-4 py-2 text-white disabled:opacity-40"
            >
              {mode.kind === "setup" ? t("totp.confirm") : t("totp.disable")}
            </button>
            <button
              type="button"
              onClick={() => setMode({ kind: "idle" })}
              className="rounded-lg px-4 py-2 text-neutral-600 hover:bg-neutral-100"
            >
              {t("nav.cancel")}
            </button>
          </div>
        </form>
      )}
      {error && <p className="text-sm text-red-600">{error}</p>}
    </section>
  );
}
