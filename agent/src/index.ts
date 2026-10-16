// Ownmind agent：由 server 拉起的无状态子进程，经 stdio JSON-RPC 通信（决策 002）。
import { createInterface } from "node:readline";
import { startAgent } from "./agent.js";
import { useFaux } from "./faux.js";
import { createModelCollection, resolveModel } from "./models.js";
import { Peer } from "./rpc.js";

const peer = new Peer((line) => process.stdout.write(`${line}\n`));
const models = createModelCollection();
const faux = process.env.OWNMIND_TEST_FAUX === "1" ? useFaux(models) : null;
if (faux) process.stderr.write("OWNMIND_TEST_FAUX=1：使用 faux 模型\n");
startAgent({
  peer,
  models,
  resolve: (spec) => faux?.() ?? resolveModel(models, spec),
});

createInterface({ input: process.stdin }).on("line", (line) => {
  peer.handleLine(line).catch((err) => {
    process.stderr.write(`处理消息失败：${(err as Error).message}\n`);
  });
});
process.stdin.on("end", () => process.exit(0));
