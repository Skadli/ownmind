import { useEffect, useState } from "react";
import { api, type Conversation } from "./api/client";
import { t } from "./i18n";
import { AccountPage } from "./pages/AccountPage";
import { ChatPage } from "./pages/ChatPage";
import { ConversationsPage } from "./pages/ConversationsPage";
import { PasswordPage } from "./pages/PasswordPage";
import { SecuritySection } from "./pages/SecuritySection";

type Screen =
  | "loading"
  | "setup"
  | "login"
  | "home"
  | "settings"
  | { chat: Conversation };

export function App() {
  const [screen, setScreen] = useState<Screen>("loading");

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
          onLoggedOut={() => setScreen("login")}
          onOpenSettings={() => setScreen("settings")}
          onOpen={(c) => setScreen({ chat: c })}
        />
      );
    case "settings":
      return (
        <main className="mx-auto max-w-2xl space-y-6 px-4 py-10">
          <button
            type="button"
            onClick={() => setScreen("home")}
            className="text-sm text-neutral-500 hover:text-neutral-900"
          >
            ← {t("nav.back")}
          </button>
          <AccountPage onUnauthenticated={() => setScreen("login")} />
          <SecuritySection onUnauthenticated={() => setScreen("login")} />
        </main>
      );
  }
}
