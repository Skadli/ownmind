// 把 server 传来的 ModelSpec 解析成 pi-ai 的模型（决策 008）。
import {
  type Api,
  createProvider,
  type Model,
  type MutableModels,
} from "@earendil-works/pi-ai";
import { anthropicMessagesApi } from "@earendil-works/pi-ai/api/anthropic-messages.lazy";
import { openAICompletionsApi } from "@earendil-works/pi-ai/api/openai-completions.lazy";
import { builtinModels } from "@earendil-works/pi-ai/providers/all";
import type { ModelSpec } from "./protocol/generated/protocol.js";

const OPENAI_COMPATIBLE = "ownmind-openai-compatible";
const ANTHROPIC_COMPATIBLE = "ownmind-anthropic-compatible";

/** 内置供应商在 pi-ai 里的 id。 */
const BUILTIN: Record<string, string> = {
  openai: "openai",
  anthropic: "anthropic",
  google: "google",
  deepseek: "deepseek",
  openrouter: "openrouter",
};

/** Key 每次请求由 server 给，provider 本身不带凭据。 */
const keyless = {
  apiKey: { name: "Ownmind", resolve: async () => ({ auth: {} }) },
};

/** 全部内置供应商 + 两个兼容地址供应商。 */
export function createModelCollection(): MutableModels {
  const models = builtinModels();
  models.setProvider(
    createProvider({
      id: OPENAI_COMPATIBLE,
      auth: keyless,
      models: [],
      api: openAICompletionsApi(),
    }),
  );
  models.setProvider(
    createProvider({
      id: ANTHROPIC_COMPATIBLE,
      auth: keyless,
      models: [],
      api: anthropicMessagesApi(),
    }),
  );
  return models;
}

function custom(
  provider: string,
  api: Api,
  id: string,
  baseUrl: string,
): Model<Api> {
  return {
    id,
    name: id,
    api,
    provider,
    baseUrl,
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128_000,
    maxTokens: 16_000,
    compat: { supportsDeveloperRole: false, supportsReasoningEffort: false },
  } as Model<Api>;
}

export function resolveModel(
  models: MutableModels,
  spec: ModelSpec,
): Model<Api> {
  if (
    spec.kind === "openai-compatible" ||
    spec.kind === "anthropic-compatible"
  ) {
    if (!spec.baseUrl) throw new Error("兼容地址缺少 baseUrl");
    return spec.kind === "openai-compatible"
      ? custom(
          OPENAI_COMPATIBLE,
          "openai-completions",
          spec.modelId,
          spec.baseUrl,
        )
      : custom(
          ANTHROPIC_COMPATIBLE,
          "anthropic-messages",
          spec.modelId,
          spec.baseUrl,
        );
  }
  const provider = BUILTIN[spec.kind];
  if (!provider) throw new Error(`未知的供应商：${spec.kind}`);
  const known = models.getModel(provider, spec.modelId);
  // 目录里没有的新模型：沿用该供应商第一个模型的 API 与地址。
  const template = known ?? models.getModels(provider)[0];
  if (!template) throw new Error(`pi-ai 目录里没有供应商 ${provider}`);
  const model = known ?? { ...template, id: spec.modelId, name: spec.modelId };
  return spec.baseUrl ? { ...model, baseUrl: spec.baseUrl } : model;
}
