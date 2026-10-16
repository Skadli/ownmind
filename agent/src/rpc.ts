// NDJSON JSON-RPC 2.0 对端：一行一条消息，双向请求/通知（决策 002）。

type Handler = (params: unknown) => unknown | Promise<unknown>;
type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

export class Peer {
  private nextId = 1;
  private readonly pending = new Map<string, Pending>();
  private readonly handlers = new Map<string, Handler>();

  constructor(private readonly write: (line: string) => void) {}

  on(method: string, handler: Handler): void {
    this.handlers.set(method, handler);
  }

  request<T>(method: string, params: unknown): Promise<T> {
    const id = `a${this.nextId++}`;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (v) => resolve(v as T),
        reject,
      });
      this.send({ id, method, params });
    });
  }

  notify(method: string, params: unknown): void {
    this.send({ method, params });
  }

  /** 处理对端发来的一行。 */
  async handleLine(line: string): Promise<void> {
    if (!line.trim()) return;
    const msg = JSON.parse(line);
    if (typeof msg.method !== "string") {
      const p = this.pending.get(String(msg.id));
      if (!p) return;
      this.pending.delete(String(msg.id));
      if (msg.error) p.reject(new Error(msg.error.message ?? "rpc error"));
      else p.resolve(msg.result);
      return;
    }
    const handler = this.handlers.get(msg.method);
    if (msg.id === undefined || msg.id === null) {
      await handler?.(msg.params);
      return;
    }
    if (!handler) {
      this.send({
        id: msg.id,
        error: { code: -32601, message: `未知方法：${msg.method}` },
      });
      return;
    }
    try {
      this.send({ id: msg.id, result: (await handler(msg.params)) ?? {} });
    } catch (err) {
      this.send({
        id: msg.id,
        error: { code: -32000, message: (err as Error).message },
      });
    }
  }

  private send(msg: Record<string, unknown>): void {
    this.write(JSON.stringify({ jsonrpc: "2.0", ...msg }));
  }
}
