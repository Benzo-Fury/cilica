import { type QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MotionConfig } from "motion/react";
import type { ReactNode } from "react";
import { Toaster } from "@/lib/toast";
import { TooltipProvider } from "@/ui";
import { useBackendEventBridge } from "./backendEvents";
import { useKeybindDispatcher } from "./keybinds";
import { createQueryClient } from "./queryClient";
import { ThemeController } from "./theme/ThemeController";

const queryClient = createQueryClient();
if (import.meta.env.DEV && typeof window !== "undefined") {
  // Dev aid: inspect the cache from the console.
  (window as unknown as { __marginQuery?: QueryClient }).__marginQuery = queryClient;
}

/** Global wiring that renders nothing: backend events, keybinds, theme. */
function AppEffects() {
  useBackendEventBridge();
  useKeybindDispatcher();
  return <ThemeController />;
}

/** The same without the app's keybinds, for windows that have no notes to act on (setup). */
function WindowEffects() {
  useBackendEventBridge();
  return <ThemeController />;
}

export function Providers({
  children,
  client = queryClient,
  keybinds = true,
}: {
  children: ReactNode;
  client?: QueryClient;
  /** False in the setup window: ⌘K and friends belong to the main window. */
  keybinds?: boolean;
}) {
  return (
    <QueryClientProvider client={client}>
      <MotionConfig reducedMotion="user">
        <TooltipProvider>
          {keybinds ? <AppEffects /> : <WindowEffects />}
          {children}
          <Toaster />
        </TooltipProvider>
      </MotionConfig>
    </QueryClientProvider>
  );
}
