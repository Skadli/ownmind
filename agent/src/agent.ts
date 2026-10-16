// agent 主逻辑：处理 server 的请求，流式调用模型并推送事件（决策 002、008）。
import type {
  Api,
  AssistantMessage,
  Context,
  Model,
  MutableModels,
} from "@earendil-works/pi-ai";
import type {
  AgentEvent,
  ContextBuildResult,
  InitializeParams,
  InitializeResult,
  ModelSpec,
  RunCancelParams,
  RunStartParams,
} from "./protocol/generated/protocol.js";
import type { Peer } from "./rpc.js";

export const PROTOCOL_VERSION = 1;
export const AGENT_VERSION = "0.0.0";

export type AgentDeps = {
  peer: Peer;
  models: MutableModels;
  resolve: (spec: ModelSpec) => Model<Api>;
};

function textOf(message: AssistantMessage): string {
  return message.content
    .flatMap((b) => (b.type === "text" ? [b.text] : []))
    .join("");
}

export function startAgent({ peer, models, resolve }: AgentDeps): {
  idle: () => Promise<void>;
} {
  const runs = new Map<string, AbortController>();
  const tasks = new Set<Promise<void>>();

  peer.on("initialize", (p): InitializeResult => {
    const { protocolVersion } = p as InitializeParams;
    if (protocolVersion !== PROTOCOL_VERSION)
      process.stderr.write(
        `协议版本不符：server ${protocolVersion}，agent ${PROTOCOL_VERSION}\n`,
      );
    return { protocolVersion: PROTOCOL_VERSION, agentVersion: AGENT_VERSION };
  });

  peer.on("run.cancel", (p) => {
    runs.get((p as RunCancelParams).runId)?.abort();
    return {};
  });

  peer.on("run.start", (p) => {
    const params = p as RunStartParams;
    const controller = new AbortController();
    runs.set(params.runId, controller);
    const task = run(params, controller.signal).finally(() => {
      runs.delete(params.runId);
      tasks.delete(task);
    });
    tasks.add(task);
    return {};
  });

  const emit = (runId: string, event: AgentEvent) =>
    peer.notify("run.event", { runId, event });

  async function run(params: RunStartParams, signal: AbortSignal) {
    const { runId } = params;
    try {
      const model = resolve(params.model);
      const ctx = await peer.request<ContextBuildResult>("context.build", {
        conversationId: params.conversationId,
      });
      const context = {
        systemPrompt: ctx.systemPrompt,
        messages: ctx.messages,
      } as Context;
      const stream = models.stream(model, context, {
        apiKey: params.model.apiKey,
        sessionId: params.conversationId,
        signal,
      });
      for await (const ev of stream) {
        if (ev.type === "text_delta")
          emit(runId, { type: "text_delta", delta: ev.delta });
        else if (ev.type === "thinking_delta")
          emit(runId, { type: "thinking_delta", delta: ev.delta });
        else if (ev.type === "done")
          emit(runId, {
            type: "done",
            message: ev.message,
            text: textOf(ev.message),
            usage: {
              input: ev.message.usage.input,
              output: ev.message.usage.output,
            },
          });
        else if (ev.type === "error")
          emit(runId, {
            type: "error",
            message: ev.error.errorMessage ?? ev.reason,
          });
      }
    } catch (err) {
      emit(runId, { type: "error", message: (err as Error).message });
    }
  }

  return {
    idle: async () => {
      while (tasks.size) await Promise.all([...tasks]);
    },
  };
}
