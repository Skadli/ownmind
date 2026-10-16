// 测试开关（OWNMIND_TEST_FAUX=1）：所有模型都换成 pi-ai 的 faux 供应商，不联网、不花钱。
// 只用于 CI 冒烟与长对话测试，回复为「faux: <主人最后一句>」。
import {
  type Api,
  type FauxResponseFactory,
  fauxAssistantMessage,
  fauxProvider,
  type Model,
  type MutableModels,
} from "@earendil-works/pi-ai";

export function useFaux(models: MutableModels): () => Model<Api> {
  const window = Number(process.env.OWNMIND_TEST_FAUX_WINDOW ?? 128_000);
  const faux = fauxProvider({
    models: [{ id: "faux", contextWindow: window, maxTokens: 1000 }],
  });
  models.setProvider(faux.provider);
  const reply: FauxResponseFactory = (context) => {
    faux.appendResponses([reply]);
    const last = [...context.messages].reverse().find((m) => m.role === "user");
    const content = last?.content;
    const text =
      typeof content === "string"
        ? content
        : (content ?? [])
            .flatMap((b) => (b.type === "text" ? [b.text] : []))
            .join("");
    return fauxAssistantMessage(`faux: ${text.split("\n")[0]}`);
  };
  faux.setResponses([reply]);
  return () => faux.getModel() as Model<Api>;
}
