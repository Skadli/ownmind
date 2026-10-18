// agent 主逻辑：处理 server 的请求，流式调用模型并推送事件（决策 002、008）。
import type {
  Api,
  AssistantMessage,
  Context,
  Model,
  MutableModels,
  ToolCall,
  ToolResultMessage,
} from "@earendil-works/pi-ai";
import type {
  AgentEvent,
  CompactionRequest,
  ContextBuildResult,
  InitializeParams,
  InitializeResult,
  ModelSpec,
  RunCancelParams,
  RunStartParams,
  ToolOutput,
} from "./protocol/generated/protocol.js";
import type { Peer } from "./rpc.js";

export const PROTOCOL_VERSION = 1;
export const AGENT_VERSION = "0.0.0";

const SUMMARY_PROMPT = `你在为一段长对话写压缩摘要，摘要会代替原文继续发给助手。
用对话所用的语言，按下面四节输出，每节用要点列出，没有内容写“无”：
## 目标
## 已做的决定
## 产出（文件、链接、关键数据）
## 未完成事项
只写对后续对话有用的事实，不要寒暄，不要编造。`;

export type AgentDeps = {
  peer: Peer;
  models: MutableModels;
  resolve: (spec: ModelSpec) => Model<Api>;
};

/** 单次运行最多的工具调用步数。 */
const MAX_STEPS = 50;

/** 供应商的 input 不含缓存命中部分；上下文大小要算全部提示 token。 */
function usageOf(message: AssistantMessage) {
  const u = message.usage;
  return { input: u.input + u.cacheRead + u.cacheWrite, output: u.output };
}

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
      const build = () =>
        peer.request<ContextBuildResult>("context.build", {
          conversationId: params.conversationId,
          contextWindow: model.contextWindow,
        });
      let ctx = await build();
      if (ctx.compaction) {
        // 压缩失败不影响本次回复：照常用完整上下文继续。
        const summary = await summarize(
          model,
          params,
          ctx.compaction,
          signal,
        ).catch((err) => {
          process.stderr.write(`压缩失败：${(err as Error).message}\n`);
          return null;
        });
        if (summary) {
          await peer.request("context.compact", {
            conversationId: params.conversationId,
            uptoSeq: ctx.compaction.uptoSeq,
            summary,
          });
          ctx = await build();
        }
      }
      const context = {
        systemPrompt: ctx.systemPrompt,
        messages: ctx.messages,
        tools: ctx.tools.map((t) => ({
          name: t.name,
          description: t.description,
          parameters: t.parameters,
        })),
      } as Context;
      for (let step = 0; ; step++) {
        if (step >= MAX_STEPS)
          throw new Error(`工具调用超过 ${MAX_STEPS} 步，已停止`);
        const message = await streamOnce(model, context, params, signal);
        if (!message) return;
        const usage = usageOf(message);
        const calls = message.content.filter(
          (b): b is ToolCall => b.type === "toolCall",
        );
        if (message.stopReason !== "toolUse" || calls.length === 0) {
          emit(runId, { type: "done", message, text: textOf(message), usage });
          return;
        }
        emit(runId, { type: "step", message, text: textOf(message), usage });
        context.messages.push(message);
        for (const call of calls) {
          emit(runId, {
            type: "tool_start",
            toolCallId: call.id,
            name: call.name,
            arguments: call.arguments,
          });
          const out = await peer.request<ToolOutput>("tool.call", {
            runId,
            conversationId: params.conversationId,
            name: call.name,
            arguments: call.arguments,
          });
          const result: ToolResultMessage = {
            role: "toolResult",
            toolCallId: call.id,
            toolName: call.name,
            content: [{ type: "text", text: out.content }],
            isError: out.isError,
            timestamp: Date.now(),
          };
          emit(runId, {
            type: "tool_end",
            toolCallId: call.id,
            name: call.name,
            message: result,
            text: out.content,
            isError: out.isError,
          });
          context.messages.push(result);
          if (signal.aborted) throw new Error("已中止");
        }
      }
    } catch (err) {
      emit(runId, { type: "error", message: (err as Error).message });
    }
  }

  /** 流式请求一次模型，转发增量；出错时发 error 事件并返回 null。 */
  async function streamOnce(
    model: Model<Api>,
    context: Context,
    params: RunStartParams,
    signal: AbortSignal,
  ): Promise<AssistantMessage | null> {
    const stream = models.stream(model, context, {
      apiKey: params.model.apiKey,
      sessionId: params.conversationId,
      signal,
    });
    for await (const ev of stream) {
      if (ev.type === "text_delta")
        emit(params.runId, { type: "text_delta", delta: ev.delta });
      else if (ev.type === "thinking_delta")
        emit(params.runId, { type: "thinking_delta", delta: ev.delta });
      else if (ev.type === "done") return ev.message;
      else if (ev.type === "error") {
        emit(params.runId, {
          type: "error",
          message: ev.error.errorMessage ?? ev.reason,
        });
        return null;
      }
    }
    throw new Error("模型流意外结束");
  }

  /** 用当前模型把较早的对话总结成结构化摘要（决策 005）。 */
  async function summarize(
    model: Model<Api>,
    params: RunStartParams,
    req: CompactionRequest,
    signal: AbortSignal,
  ): Promise<string> {
    const previous = req.previousSummary
      ? `已有摘要（新摘要要包含其中仍然有效的内容）：\n${req.previousSummary}\n\n`
      : "";
    const context = {
      systemPrompt: SUMMARY_PROMPT,
      messages: [
        {
          role: "user",
          content: `${previous}需要总结的对话：\n\n${req.transcript}`,
          timestamp: Date.now(),
        },
      ],
    } as Context;
    const stream = models.stream(model, context, {
      apiKey: params.model.apiKey,
      signal,
    });
    for await (const ev of stream) {
      if (ev.type === "done") return textOf(ev.message).trim();
      if (ev.type === "error")
        throw new Error(ev.error.errorMessage ?? ev.reason);
    }
    throw new Error("摘要流意外结束");
  }

  return {
    idle: async () => {
      while (tasks.size) await Promise.all([...tasks]);
    },
  };
}
