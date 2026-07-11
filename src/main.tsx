import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components";

// Global observability for otherwise-silent async failures. Streaming and IPC
// calls that reject without a local catch would vanish without these — logging
// them makes intermittent field issues diagnosable.
window.addEventListener("unhandledrejection", (e) => {
  console.error("[unhandledrejection]", e.reason);
});
window.addEventListener("error", (e) => {
  console.error("[window error]", e.error ?? e.message);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
