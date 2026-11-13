/**
 * The first-run setup window (`?window=onboarding`, shell/onboarding.rs). A welcome screen that
 * says what Cilica is (a notes app with an AI assistant and a meeting summariser), then
 * five steps in one column over background art: permissions, the speech model, the assistant's
 * model, imports, and appearance, whose button opens Cilica. Every step can be skipped and
 * closing the window opens Cilica with whatever was set up.
 */
import { useMutation } from "@tanstack/react-query";
import {
  ArrowRight,
  AudioLines,
  FileText,
  FolderInput,
  Sparkles,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useEffect, useReducer, useRef, useState } from "react";
import { ErrorBoundary } from "@/app/ErrorBoundary";
import { isMac } from "@/app/platform";
import { Providers } from "@/app/providers";
import { ImportWatcher } from "@/features/import/ImportPage";
import { useModelDownloadEvents } from "@/features/settings/system";
import { useWindowControls } from "@/features/shell/useWindowControls";
import { api, errorMessage, isTauri } from "@/ipc/client";
import { toast } from "@/lib/toast";
import {
  Button,
  Icon,
  IconButton,
  SceneArt,
  SetupBand,
  type SetupFeature,
  SetupFrame,
  SetupSteps,
  SetupWelcome,
  TrafficLightsSpace,
  WindowControls,
} from "@/ui";
import { BackgroundTasks } from "./BackgroundTasks";
import { flowReducer, initialFlow, STEPS, stepCount, stepStatus } from "./flow";
import { ImportStep } from "./ImportStep";
import { LookStep } from "./LookStep";
import { ModelStep } from "./ModelStep";
import { PermissionsStep } from "./PermissionsStep";
import { SpeechStep } from "./SpeechStep";
import { THIS_DEVICE } from "./shared";
import { useAmbientSound } from "./useAmbientSound";

/**
 * What Cilica is, before it asks for anything, in the order the lead names it: a notes app, its
 * AI assistant, its meeting summariser. Then the way in for notes kept elsewhere. From the README.
 */
const FEATURES: readonly SetupFeature[] = [
  {
    icon: FileText,
    title: "Notes in plain Markdown",
    text: `Write and sort them into categories. They stay on ${THIS_DEVICE}, yours to keep.`,
  },
  {
    icon: Sparkles,
    title: "An AI assistant in your notes",
    text: "Cilica can sort, summarise or tidy them up. Every change can be undone.",
  },
  {
    icon: AudioLines,
    title: "Meeting summaries, no bot",
    text: `Calls are transcribed on ${THIS_DEVICE} and written up with decisions and action items.`,
  },
  {
    icon: FolderInput,
    title: "Bring notes from 45 apps",
    text: "Including Obsidian, Notion, Apple Notes and Granola.",
  },
];

/** How long the welcome takes to lift away before the steps come in (`--dur-fast` + a frame). */
const LEAVE_MS = 170;
/** Let a step finish opening before scrolling to it (`--spring-smooth-dur`). */
const SCROLL_AFTER_MS = 340;

export function OnboardingWindow() {
  return (
    <ErrorBoundary>
      <Providers keybinds={false}>
        <Setup />
        <ImportWatcher />
      </Providers>
    </ErrorBoundary>
  );
}

function useFinish(onDone: () => void) {
  return useMutation({
    mutationFn: () => api.finishOnboarding(),
    onSuccess: () => {
      onDone();
      // The browser preview has no main window to show: go to it instead.
      if (!isTauri) window.location.assign("/");
    },
    onError: (error) =>
      toast.error("Cilica did not open", {
        description: `${errorMessage(error)} Close this window to open Cilica anyway.`,
      }),
  });
}

function Setup() {
  useModelDownloadEvents();
  const [flow, dispatch] = useReducer(flowReducer, initialFlow);
  const [ready, setReady] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const stepsRef = useRef<HTMLDivElement>(null);
  const sound = useAmbientSound();
  const finish = useFinish(sound.fadeOut);
  const controls = useWindowControls();
  const reduced =
    typeof window !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Start the art's rise once the first frame is on screen. A timer, not rAF: the window is
  // still hidden when it first renders, and hidden windows get no animation frames.
  useEffect(() => {
    const timer = setTimeout(() => setReady(true), 30);
    return () => clearTimeout(timer);
  }, []);

  // Bring the open step into view once it has opened.
  const current = STEPS[flow.current];
  useEffect(() => {
    if (flow.phase !== "steps") return;
    const timer = setTimeout(() => {
      const box = stepsRef.current;
      const el = box?.querySelector<HTMLElement>(`[data-step="${current}"]`);
      if (box && el) box.scrollTo({ top: el.offsetTop - 8, behavior: reduced ? "auto" : "smooth" });
    }, SCROLL_AFTER_MS);
    return () => clearTimeout(timer);
  }, [flow.phase, current, reduced]);

  const start = () => {
    setLeaving(true);
    setTimeout(
      () => {
        dispatch({ type: "start" });
        setLeaving(false);
      },
      reduced ? 0 : LEAVE_MS,
    );
  };

  const step = (index: number) => {
    const status = stepStatus(flow, index);
    const id = STEPS[index];
    return status === "upcoming" || !id
      ? null
      : {
          status,
          skipped: Boolean(flow.skipped[id]),
          onNext: (skipped: boolean) => dispatch({ type: "next", skipped }),
          onReopen: () => dispatch({ type: "reopen", index }),
        };
  };
  const [permissions, speech, model, imports, look] = STEPS.map((_, i) => step(i));

  return (
    <SetupFrame
      phase={flow.phase}
      ready={ready}
      art={<SceneArt />}
      band={
        <SetupBand
          start={isMac ? <TrafficLightsSpace /> : <span />}
          title="Set up Cilica"
          count={stepCount(flow)}
          end={
            <>
              <IconButton
                icon={sound.playing ? Volume2 : VolumeX}
                label={sound.playing ? "Mute background sound" : "Play background sound"}
                size="sm"
                onClick={sound.toggle}
              />
              {!isMac && <WindowControls onClose={controls.onClose} />}
            </>
          }
        />
      }
      tray={<BackgroundTasks />}
    >
      {flow.phase === "welcome" ? (
        <SetupWelcome
          title={
            <>
              Welcome to <em>Cilica</em>
            </>
          }
          lead="A notes app with an AI assistant and a meeting summariser."
          features={FEATURES}
          leaving={leaving}
          action={
            <Button variant="primary" size="lg" autoFocus onClick={start}>
              Set up Cilica
              <Icon icon={ArrowRight} size="sm" motion="arrow" />
            </Button>
          }
          note="Takes about 2 minutes. You can skip any step."
        />
      ) : (
        <SetupSteps ref={stepsRef}>
          {permissions && <PermissionsStep {...permissions} />}
          {speech && <SpeechStep {...speech} />}
          {model && <ModelStep {...model} />}
          {imports && <ImportStep {...imports} />}
          {look && (
            <LookStep
              status={look.status}
              onReopen={look.onReopen}
              finishing={finish.isPending}
              onFinish={() => finish.mutate()}
            />
          )}
        </SetupSteps>
      )}
    </SetupFrame>
  );
}
