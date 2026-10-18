import {
  type FormEvent,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ApiError, api, type Conversation, type Message } from "../api/client";
import { errorText, t } from "../i18n";

type Props = {
  conversation: Conversation;
  onBack: () => void;
  onLoggedOut: () => void;
  onOpenSettings: () => void;
};

/** 正在生成的回复。 */
type Draft = { text: string; thinking: boolean; tool?: string };

export function ChatPage({
  conversation,
  onBack,
  onLoggedOut,
  onOpenSettings,
}: Props) {
  const id = conversation.id;
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const bottom = useRef<HTMLDivElement>(null);

  const fail = useCallback(
    (err: unknown) => {
      const code = err instanceof ApiError ? err.code : "internal";
      if (code === "unauthenticated") return onLoggedOut();
      setError(code);
    },
    [onLoggedOut],
  );

  const reload = useCallback(
    () => api.listMessages(id).then(setMessages, fail),
    [id, fail],
  );

  // 打开时加载历史；如果上次的回复还在跑（浏览器断开过），等它结束再刷新。
  useEffect(() => {
    let cancelled = false;
    reload();
    (async () => {
      while (
        !cancelled &&
        (await api.runStatus(id).catch(() => null))?.running
      ) {
        setPending(true);
        await new Promise((r) => setTimeout(r, 1000));
      }
      if (!cancelled) {
        setPending(false);
        reload();
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [id, reload]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: 消息或草稿变化时滚到底部
  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [messages, draft]);

  const send = async (e?: FormEvent) => {
    e?.preventDefault();
    const text = input.trim();
    if (!text || draft || pending) return;
    setError(null);
    setDraft({ text: "", thinking: true });
    try {
      await api.sendMessage(id, text, (ev) => {
        switch (ev.type) {
          case "user":
            setInput("");
            setMessages((m) => [...m, ev.message]);
            break;
          case "thinking":
            break;
          case "delta":
            setDraft((d) => ({
              text: (d?.text ?? "") + ev.delta,
              thinking: false,
            }));
            break;
          case "saved":
            setMessages((m) => [...m, ev.message]);
            setDraft({ text: "", thinking: true });
            break;
          case "tool_start":
            setDraft({ text: "", thinking: true, tool: ev.name });
            break;
          case "done":
            setMessages((m) => [...m, ev.message]);
            setDraft(null);
            break;
          case "error":
            setError(ev.message);
            break;
        }
      });
    } catch (err) {
      fail(err);
    } finally {
      setDraft(null);
    }
  };

  const stop = () => api.stopRun(id).catch(fail);

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      send();
    }
  };

  const busy = draft !== null || pending;

  return (
    <main className="mx-auto flex h-dvh max-w-3xl flex-col px-4">
      <header className="flex items-center gap-3 py-4">
        <button
          type="button"
          onClick={onBack}
          className="text-sm text-neutral-500 hover:text-neutral-900"
        >
          ← {t("nav.back")}
        </button>
        <h1 className="flex-1 truncate text-lg font-semibold">
          {conversation.title}
        </h1>
      </header>

      <div className="flex-1 space-y-4 overflow-y-auto pb-4">
        {messages.length === 0 && !draft && (
          <p className="pt-10 text-center text-neutral-500">
            {t("chat.empty")}
          </p>
        )}
        {messages.map((m) => (
          <Bubble key={m.id} from={m.role} text={m.text} />
        ))}
        {draft &&
          (draft.thinking ? (
            <p className="text-sm text-neutral-400">
              {draft.tool
                ? t("chat.tool").replace("{name}", draft.tool)
                : t("chat.thinking")}
            </p>
          ) : (
            <Bubble from="assistant" text={draft.text} />
          ))}
        {pending && !draft && (
          <p className="text-sm text-neutral-400">{t("chat.running")}</p>
        )}
        <div ref={bottom} />
      </div>

      {error && (
        <p className="pb-2 text-sm text-red-600">
          {errorText(error) === errorText("internal")
            ? error
            : errorText(error)}
          {error === "no_default_model" && (
            <button
              type="button"
              onClick={onOpenSettings}
              className="ml-2 font-medium underline"
            >
              {t("chat.goSettings")}
            </button>
          )}
        </p>
      )}

      <form onSubmit={send} className="flex items-end gap-2 pb-6">
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
          rows={Math.min(8, input.split("\n").length)}
          placeholder={t("chat.placeholder")}
          className="flex-1 resize-none rounded-2xl border border-neutral-300 bg-white px-4 py-3 shadow-sm focus:outline-none"
        />
        {busy ? (
          <button
            type="button"
            onClick={stop}
            className="rounded-2xl border border-neutral-300 px-4 py-3"
          >
            {t("chat.stop")}
          </button>
        ) : (
          <button
            type="submit"
            disabled={!input.trim()}
            className="rounded-2xl bg-neutral-900 px-4 py-3 text-white disabled:opacity-40"
          >
            {t("chat.send")}
          </button>
        )}
      </form>
    </main>
  );
}

function Bubble({ from, text }: { from: Message["role"]; text: string }) {
  if (from === "tool")
    return (
      <details className="text-sm text-neutral-500">
        <summary className="cursor-pointer">{text.slice(0, 80)}</summary>
        <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded bg-neutral-100 p-2">
          {text}
        </pre>
      </details>
    );
  // 只含工具调用、没有文字的回复不显示气泡。
  if (from === "assistant" && !text.trim()) return null;
  if (from === "user")
    return (
      <div className="flex justify-end">
        <p className="max-w-[85%] whitespace-pre-wrap rounded-2xl bg-neutral-900 px-4 py-2 text-white">
          {text}
        </p>
      </div>
    );
  return (
    <div className="markdown max-w-none leading-relaxed text-neutral-900">
      <Markdown remarkPlugins={[remarkGfm]}>{text}</Markdown>
    </div>
  );
}
