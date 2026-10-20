import { describe, expect, it } from "vitest";
import { argSummary } from "./toolSummary";

describe("argSummary", () => {
  it("优先取命令并压成一行", () => {
    expect(argSummary({ command: "ls\n  -la", path: "/x" })).toBe("ls -la");
  });
  it("没有参数返回空串", () => {
    expect(argSummary(undefined)).toBe("");
    expect(argSummary({})).toBe("");
  });
  it("支持查询与网址并截短", () => {
    expect(argSummary({ query: "rust" })).toBe("rust");
    expect(argSummary({ url: "a".repeat(100) })).toHaveLength(80);
  });
});
