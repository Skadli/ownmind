import { describe, expect, it } from "vitest";
import { intervalSecs, localToMs, splitInterval } from "./schedule";

describe("schedule helpers", () => {
  it("数量和单位换成秒", () => {
    expect(intervalSecs(2, "hours")).toBe(7200);
    expect(intervalSecs(1, "days")).toBe(86400);
  });
  it("非正整数无效", () => {
    expect(intervalSecs(0, "minutes")).toBeNull();
    expect(intervalSecs(1.5, "minutes")).toBeNull();
    expect(intervalSecs(Number.NaN, "minutes")).toBeNull();
  });
  it("拆分回最大整除单位", () => {
    expect(splitInterval(172800)).toEqual({ amount: 2, unit: "days" });
    expect(splitInterval(7200)).toEqual({ amount: 2, unit: "hours" });
    expect(splitInterval(90)).toEqual({ amount: 2, unit: "minutes" });
  });
  it("本地时间解析", () => {
    expect(localToMs("not a date")).toBeNull();
    expect(localToMs("2026-10-20T08:00")).not.toBeNull();
  });
});
