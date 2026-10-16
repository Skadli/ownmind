// 解析 POST 返回的 SSE 流（EventSource 只支持 GET，所以自己读）。

/** 把一段段文本切成 SSE 事件，返回每个事件的 data 字段（多行已拼接）。 */
export class SseParser {
  private buffer = "";

  push(chunk: string): string[] {
    this.buffer += chunk.replace(/\r\n/g, "\n");
    const out: string[] = [];
    let end = this.buffer.indexOf("\n\n");
    while (end >= 0) {
      const block = this.buffer.slice(0, end);
      this.buffer = this.buffer.slice(end + 2);
      const data = block
        .split("\n")
        .filter((l) => l.startsWith("data:"))
        .map((l) => l.slice(5).replace(/^ /, ""));
      if (data.length) out.push(data.join("\n"));
      end = this.buffer.indexOf("\n\n");
    }
    return out;
  }
}
