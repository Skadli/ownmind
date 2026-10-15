import { useEffect, useState } from "react";
import { api } from "./api/client";
import { ConversationsPage } from "./pages/ConversationsPage";
import { PasswordPage } from "./pages/PasswordPage";

type Screen = "loading" | "setup" | "login" | "home";

export function App() {
  const [screen, setScreen] = useState<Screen>("loading");

  useEffect(() => {
    api
      .status()
      .then((s) =>
        setScreen(!s.isSetUp ? "setup" : s.isLoggedIn ? "home" : "login"),
      );
  }, []);

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
          onSubmit={async (pw) => {
            await api.logIn(pw);
            setScreen("home");
          }}
        />
      );
    case "home":
      return <ConversationsPage onLoggedOut={() => setScreen("login")} />;
  }
}
