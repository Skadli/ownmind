// 测试用假 agent：按协议应答，不连模型。环境变量控制异常行为。
import { createInterface } from "node:readline";

const version = Number(process.env.FAKE_VERSION ?? 1);
const crashOnRun = process.env.FAKE_CRASH === "1";
let nextId = 1;
const pending = new Map();
const send = (m) =>
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...m })}\n`);
const call = (method, params) =>
  new Promise((resolve, reject) => {
    const id = `a${nextId++}`;
    pending.set(id, { resolve, reject });
    send({ id, method, params });
  });

createInterface({ input: process.stdin }).on("line", async (line) => {
  const msg = JSON.parse(line);
  if (!msg.method) {
    const p = pending.get(msg.id);
    pending.delete(msg.id);
    return msg.error
      ? p.reject(new Error(msg.error.message))
      : p.resolve(msg.result);
  }
  if (msg.method === "initialize")
    return send({
      id: msg.id,
      result: { protocolVersion: version, agentVersion: "fake" },
    });
  if (msg.method === "run.cancel") {
    send({ id: msg.id, result: {} });
    return send({
      method: "run.event",
      params: {
        runId: msg.params.runId,
        event: { type: "error", message: "aborted" },
      },
    });
  }
  if (msg.method === "run.start") {
    if (crashOnRun) process.exit(3);
    send({ id: msg.id, result: {} });
    const { runId, conversationId, model } = msg.params;
    const contextWindow = Number(process.env.FAKE_WINDOW ?? 100000);
    let ctx = await call("context.build", { conversationId, contextWindow });
    if (ctx.compaction) {
      const { uptoSeq } = ctx.compaction;
      await call("context.compact", {
        conversationId,
        uptoSeq,
        summary: `摘要至 ${uptoSeq}`,
      });
      ctx = await call("context.build", { conversationId, contextWindow });
    }
    const ev = (event) =>
      send({ method: "run.event", params: { runId, event } });
    if (model.modelId === "slow") return; // 等 run.cancel
    if (model.modelId.startsWith("tool:")) {
      // 模拟一次工具调用：tool:<工具名>
      const name = model.modelId.slice(5);
      const toolCall = {
        type: "toolCall",
        id: "c1",
        name,
        arguments: { x: 1 },
      };
      ev({
        type: "step",
        message: {
          role: "assistant",
          content: [toolCall],
          stopReason: "toolUse",
        },
        text: "",
        usage: { input: 10, output: 2 },
      });
      ev({ type: "tool_start", toolCallId: "c1", name, arguments: { x: 1 } });
      const out = await call("tool.call", {
        runId,
        conversationId,
        name,
        arguments: { x: 1 },
      });
      ev({
        type: "tool_end",
        toolCallId: "c1",
        name,
        message: {
          role: "toolResult",
          toolCallId: "c1",
          toolName: name,
          content: [{ type: "text", text: out.content }],
          isError: out.isError,
        },
        text: out.content,
        isError: out.isError,
      });
      // 和真实 agent 一样只在运行开始时取上下文：报告开始时的条数（不在运行中途重取，
      // 否则会和 server 异步保存 step/tool 消息竞争）。
      const text = `工具说：${out.content}；工具 ${ctx.tools.map((t) => t.name).join(",")}；${ctx.messages.length} 条`;
      return ev({
        type: "done",
        message: { role: "assistant", content: [{ type: "text", text }] },
        text,
        usage: { input: 20, output: 5 },
      });
    }
    ev({ type: "text_delta", delta: `${ctx.messages.length} 条消息，` });
    ev({ type: "text_delta", delta: `模型 ${model.modelId}` });
    const text = `${ctx.messages.length} 条消息，模型 ${model.modelId}`;
    ev({
      type: "done",
      message: { role: "assistant", content: [{ type: "text", text }] },
      text,
      usage: { input: 10, output: 5 },
    });
  }
});
