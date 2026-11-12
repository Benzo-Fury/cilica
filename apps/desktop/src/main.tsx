import React from "react";
import ReactDOM from "react-dom/client";
import { api, isTauri } from "@/ipc/client";
import "@/styles/index.css";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
const isIsland = new URLSearchParams(window.location.search).get("window") === "island";

/** The main window starts hidden to avoid a white flash; show it once the first render ran. */
function showWhenRendered() {
  if (!isTauri) return;
  setTimeout(() => {
    api.showMainWindow().catch((error: unknown) => console.error("showMainWindow failed", error));
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
} else {
  void import("@/app/App")
    .then(({ App }) => {
      root.render(
        <React.StrictMode>
          <App />
        </React.StrictMode>,
      );
    })
    .finally(showWhenRendered);
}
