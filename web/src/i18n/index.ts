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

const LANG_STORAGE = "ownmind.language";

function browserLanguages(): readonly string[] {
  return typeof navigator === "undefined" ? [] : navigator.languages;
}

function initialLanguages(): readonly string[] {
  const saved =
    typeof localStorage === "undefined"
      ? null
      : localStorage.getItem(LANG_STORAGE);
  return saved && saved !== "auto" ? [saved] : browserLanguages();
}

let messages = pickMessages(initialLanguages());

/** 切换界面语言（auto 跟随浏览器），并记住以便登录页也用它。调用方负责重新渲染。 */
export function setLanguage(lang: "auto" | "zh" | "en") {
  if (typeof localStorage !== "undefined")
    localStorage.setItem(LANG_STORAGE, lang);
  messages = pickMessages(lang === "auto" ? browserLanguages() : [lang]);
}

export function t(key: MessageKey): string {
  return messages[key];
}

/** 把接口返回的错误码翻译成文案。 */
export function errorText(code: string): string {
  const key = `error.${code}` as MessageKey;
  return key in messages ? messages[key] : messages["error.internal"];
}
