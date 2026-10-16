import { describe, expect, it } from "vitest";
import { SseParser } from "./sse";

describe("SseParser", () => {
  it("splits events across chunks and ignores comments", () => {
    const p = new SseParser();
    expect(p.push(': keep-alive\n\nevent: delta\ndata: {"a"')).toEqual([]);
    expect(p.push(":1}\n\nevent: done\r\ndata: x\r\n\r\n")).toEqual([
      '{"a":1}',
      "x",
    ]);
  });

  it("joins multi-line data", () => {
    expect(new SseParser().push("data: a\ndata: b\n\n")).toEqual(["a\nb"]);
  });
});
