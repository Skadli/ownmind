import {
  createModels,
  fauxAssistantMessage,
  fauxProvider,
} from "@earendil-works/pi-ai";
import { describe, expect, it } from "vitest";
import { PROTOCOL_VERSION, startAgent } from "./agent.js";
import { createModelCollection, resolveModel } from "./models.js";
import { Peer } from "./rpc.js";

/** 模拟 server：把 agent 的输出喂回给自己，记录通知，并回答 context.build。 */
function harness() {
  const faux = fauxProvider();
  const models = createModels();
  models.setProvider(faux.provider);
  const sent: Record<string, unknown>[] = [];
  let agentPeer: Peer;
  const server = new Peer((line) => agentPeer.handleLine(line));
  agentPeer = new Peer((line) => {
    sent.push(JSON.parse(line));
    server.handleLine(line);
  });
  const compacted: unknown[] = [];
  let needsCompaction = false;
  server.on("context.build", () => ({
    systemPrompt: "sys",
    messages: [{ role: "user", content: "你好", timestamp: 0 }],
    estimatedTokens: 10,
    compaction:
      needsCompaction && compacted.length === 0
        ? { uptoSeq: 4, previousSummary: null, transcript: "[主人]\n早先" }
        : null,
  }));
  server.on("context.compact", (p) => {
    compacted.push(p);
    return {};
  });
  const agent = startAgent({
    peer: agentPeer,
    models,
    resolve: () => faux.getModel(),
  });
  const events = () =>
    sent
      .filter((m) => m.method === "run.event")
      .map((m) => (m.params as { event: { type: string } }).event);
  return {
    faux,
    server,
    agent,
    events,
    compacted,
    needCompaction: () => {
      needsCompaction = true;
    },
  };
}

const spec = { kind: "openai", baseUrl: null, apiKey: "k", modelId: "m" };

describe("agent", () => {
  it("handshakes with the protocol version", async () => {
    const { server } = harness();
    const r = await server.request<{ protocolVersion: number }>("initialize", {
      protocolVersion: PROTOCOL_VERSION,
    });
    expect(r.protocolVersion).toBe(PROTOCOL_VERSION);
  });

  it("streams deltas then done with text and usage", async () => {
    const { faux, server, agent, events } = harness();
    faux.setResponses([fauxAssistantMessage("你好呀")]);
    await server.request("run.start", {
      runId: "r1",
      conversationId: "c1",
      model: spec,
    });
    await agent.idle();
    const evs = events();
    const deltas = evs
      .filter((e) => e.type === "text_delta")
      .map((e) => (e as unknown as { delta: string }).delta)
      .join("");
    expect(deltas).toBe("你好呀");
    const done = evs.at(-1) as unknown as { type: string; text: string };
    expect(done.type).toBe("done");
    expect(done.text).toBe("你好呀");
  });

  it("reports model errors as an error event", async () => {
    const { server, agent, events } = harness();
    await server.request("run.start", {
      runId: "r2",
      conversationId: "c1",
      model: spec,
    });
    await agent.idle();
    expect(events().at(-1)?.type).toBe("error");
  });
});

describe("compaction", () => {
  it("summarizes with the model, saves it, then answers", async () => {
    const h = harness();
    h.needCompaction();
    h.faux.setResponses([
      fauxAssistantMessage("## 目标\n- 测试"),
      fauxAssistantMessage("好的"),
    ]);
    await h.server.request("run.start", {
      runId: "r3",
      conversationId: "c1",
      model: spec,
    });
    await h.agent.idle();
    expect(h.compacted).toEqual([
      { conversationId: "c1", uptoSeq: 4, summary: "## 目标\n- 测试" },
    ]);
    const done = h.events().at(-1) as unknown as { type: string; text: string };
    expect(done).toMatchObject({ type: "done", text: "好的" });
  });
});

describe("resolveModel", () => {
  const models = createModelCollection();

  it("builds compatible endpoints from the base url", () => {
    const m = resolveModel(models, {
      kind: "openai-compatible",
      baseUrl: "http://localhost:11434/v1",
      apiKey: "x",
      modelId: "qwen3",
    });
    expect([m.api, m.baseUrl, m.id]).toEqual([
      "openai-completions",
      "http://localhost:11434/v1",
      "qwen3",
    ]);
  });

  it("falls back to the provider's api for unknown model ids", () => {
    const m = resolveModel(models, {
      kind: "anthropic",
      baseUrl: null,
      apiKey: "x",
      modelId: "claude-future-9",
    });
    expect(m.provider).toBe("anthropic");
    expect(m.api).toBe("anthropic-messages");
    expect(m.id).toBe("claude-future-9");
  });
});
