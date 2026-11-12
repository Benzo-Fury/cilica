import React from "react";
import ReactDOM from "react-dom/client";
import { api, isTauri } from "@/ipc/client";
import "@/styles/index.css";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
const windowKind = new URLSearchParams(window.location.search).get("window");
const isIsland = windowKind === "island";
const isOnboarding = windowKind === "onboarding";

/** Windows start hidden to avoid a white flash; show this one once the first render ran. */
function showWhenRendered(show: () => Promise<unknown>, name: string) {
  if (!isTauri) return;
  setTimeout(() => {
    show().catch((error: unknown) => console.error(`${name} failed`, error));
  }, 0);
}

// The Dynamic Island is a second window loading this same bundle; it renders only the island.
if (isIsland) {
  void import("@/features/island/IslandWindow").then(({ IslandWindow }) =>
    root.render(
      <React.StrictMode>
        <IslandWindow />
      </React.StrictMode>,
    ),
  );
} else if (isOnboarding) {
  // First-run setup is its own window (shell/onboarding.rs); the main window waits behind it.
  void import("@/features/onboarding/OnboardingWindow")
    .then(({ OnboardingWindow }) => {
      root.render(
        <React.StrictMode>
          <OnboardingWindow />
        </React.StrictMode>,
      );
    })
    .finally(() => showWhenRendered(() => api.showOnboardingWindow(), "showOnboardingWindow"));
} else {
  void import("@/app/App")
    .then(({ App }) => {
      root.render(
        <React.StrictMode>
          <App />
        </React.StrictMode>,
      );
    })
    .finally(() => showWhenRendered(() => api.showMainWindow(), "showMainWindow"));
}
