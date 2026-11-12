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

export function Providers({
  children,
  client = queryClient,
}: {
  children: ReactNode;
  client?: QueryClient;
}) {
  return (
    <QueryClientProvider client={client}>
      <MotionConfig reducedMotion="user">
        <TooltipProvider>
          <AppEffects />
          {children}
          <Toaster />
        </TooltipProvider>
      </MotionConfig>
    </QueryClientProvider>
  );
}
