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
    const ctx = await call("context.build", { conversationId });
    const ev = (event) =>
      send({ method: "run.event", params: { runId, event } });
    if (model.modelId === "slow") return; // 等 run.cancel
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
