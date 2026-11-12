import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { initLighting } from "./lib/lighting";
import "./styles.css";

// The Lighting page's saved state comes from the backend; pages read it
// from the first render on (the window never waits long for it).
const lightingReady = Promise.race([initLighting(), new Promise((r) => setTimeout(r, 3000))]);
void lightingReady.finally(() =>
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <ErrorBoundary full>
        <App />
      </ErrorBoundary>
    </StrictMode>,
  ),
);
