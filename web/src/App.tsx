import { useEffect, useState } from "react";
import { api, type Conversation } from "./api/client";
import { setLanguage } from "./i18n";
import { ChatPage } from "./pages/ChatPage";
import { ConversationsPage } from "./pages/ConversationsPage";
import { PasswordPage } from "./pages/PasswordPage";
import { SettingsPage } from "./pages/SettingsPage";

type Screen =
  | "loading"
  | "setup"
  | "login"
  | "home"
  | "settings"
  | { chat: Conversation };

export function App() {
  const [screen, setScreen] = useState<Screen>("loading");
  const [langVersion, setLangVersion] = useState(0);
  const loggedIn =
    screen !== "loading" && screen !== "setup" && screen !== "login";

  // 登录后按系统偏好设置界面语言。
  useEffect(() => {
    if (!loggedIn) return;
    api.getSystem().then(
      (s) => {
        setLanguage(s.language);
        setLangVersion((n) => n + 1);
      },
      () => {},
    );
  }, [loggedIn]);

  useEffect(() => {
    api
      .status()
      .then((s) =>
        setScreen(!s.isSetUp ? "setup" : s.isLoggedIn ? "home" : "login"),
      );
  }, []);

  if (typeof screen === "object")
    return (
      <ChatPage
        key={langVersion}
        conversation={screen.chat}
        onBack={() => setScreen("home")}
        onLoggedOut={() => setScreen("login")}
        onOpenSettings={() => setScreen("settings")}
      />
    );

  switch (screen) {
    case "loading":
      return null;
    case "setup":
      return (
        <PasswordPage
          mode="setup"
          onSubmit={async (pw) => {
            await api.setUp(pw);
            setScreen("home");
          }}
        />
      );
    case "login":
      return (
        <PasswordPage
          mode="login"
          onSubmit={async (pw, code) => {
            await api.logIn(pw, code);
            setScreen("home");
          }}
        />
      );
    case "home":
      return (
        <ConversationsPage
          key={langVersion}
          onLoggedOut={() => setScreen("login")}
          onOpenSettings={() => setScreen("settings")}
          onOpen={(c) => setScreen({ chat: c })}
        />
      );
    case "settings":
      return (
        <SettingsPage
          key={langVersion}
          onBack={() => setScreen("home")}
          onLoggedOut={() => setScreen("login")}
          onLanguageChange={() => setLangVersion((n) => n + 1)}
        />
      );
  }
}
