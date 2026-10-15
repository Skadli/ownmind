import { describe, expect, it } from "vitest";
import { pickMessages } from ".";
import { en } from "./en";
import { zh } from "./zh";

describe("pickMessages", () => {
  it("matches the browser language by its base tag", () => {
    expect(pickMessages(["en-US", "zh-CN"])).toBe(en);
    expect(pickMessages(["zh-TW"])).toBe(zh);
  });

  it("falls back to Chinese", () => {
    expect(pickMessages(["fr-FR"])).toBe(zh);
    expect(pickMessages([])).toBe(zh);
  });
});
