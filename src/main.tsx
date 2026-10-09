import React from "react";
import ReactDOM from "react-dom/client";
import "maplibre-gl/dist/maplibre-gl.css";
import "flag-icons/css/flag-icons.min.css";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/layout.css";
import "./styles/features.css";

async function start() {
  // `npm run demo`: the UI in a normal browser with a sample library (design work, screenshots). Compiled out of real builds.
  if (import.meta.env.MODE === "demo") {
    const { installDemoBackend } = await import("./dev/demoBackend");
    const q = new URLSearchParams(location.search);
    installDemoBackend({ empty: q.has("empty"), noApiKey: q.has("nokey"), failWith: q.get("fail") ?? undefined });
    if (q.get("theme")) document.documentElement.dataset.theme = q.get("theme")!;
  }
  const { default: App } = await import("./App");
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void start();
