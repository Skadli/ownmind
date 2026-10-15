import { defineConfig } from "@rsbuild/core";
import { pluginReact } from "@rsbuild/plugin-react";

const server = process.env.OWNMIND_DEV_SERVER ?? "http://127.0.0.1:8787";

export default defineConfig({
  plugins: [pluginReact()],
  html: { title: "Ownmind" },
  server: {
    port: 5173,
    proxy: { "/api": server, "/healthz": server },
  },
});
