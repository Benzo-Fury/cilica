import { ImportWatcher } from "@/features/import/ImportPage";
import { Shell } from "@/features/shell/Shell";
import { ErrorBoundary } from "./ErrorBoundary";
import { Providers } from "./providers";

/** The main window. (main.tsx shows the hidden window once this has rendered.) */
export function App() {
  return (
    <ErrorBoundary>
      <Providers>
        <Shell />
        <ImportWatcher />
      </Providers>
    </ErrorBoundary>
  );
}
