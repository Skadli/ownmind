import { useEffect, useState } from "react";

type Health = "checking" | "ok" | "down";

export function App() {
  const [health, setHealth] = useState<Health>("checking");

  useEffect(() => {
    fetch("/healthz")
      .then((res) => setHealth(res.ok ? "ok" : "down"))
      .catch(() => setHealth("down"));
  }, []);

  return (
    <main className="flex min-h-screen flex-col items-center justify-center gap-2 bg-neutral-50 text-neutral-900">
      <h1 className="text-3xl font-semibold">Ownmind</h1>
      <p className="text-sm text-neutral-500">server: {health}</p>
    </main>
  );
}
