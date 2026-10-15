import { en } from "./en";
import { zh } from "./zh";

export type Messages = typeof zh;
export type MessageKey = keyof Messages;

const catalogs: Record<string, Messages> = { zh, en };

/** 按浏览器语言选文案；没有对应语言时用中文。 */
export function pickMessages(languages: readonly string[]): Messages {
  for (const lang of languages) {
    const found = catalogs[lang.toLowerCase().split("-")[0]];
    if (found) return found;
  }
  return zh;
}

const messages = pickMessages(
  typeof navigator === "undefined" ? [] : navigator.languages,
);

export function t(key: MessageKey): string {
  return messages[key];
}

/** 把接口返回的错误码翻译成文案。 */
export function errorText(code: string): string {
  const key = `error.${code}` as MessageKey;
  return key in messages ? messages[key] : messages["error.internal"];
}
