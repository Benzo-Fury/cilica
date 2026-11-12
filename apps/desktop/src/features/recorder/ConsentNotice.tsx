/** The one-time recording notice (spec › Consent), before the first recording or right after it. */
import { Mic, ShieldCheck } from "lucide-react";
import { useSettings } from "@/features/settings/hooks";
import {
  Button,
  DialogActions,
  DialogDescription,
  DialogRoot,
  DialogSurface,
  DialogTitle,
  Icon,
} from "@/ui";
import { useConsentActions, useRecorderUi } from "./hooks";

export function ConsentNotice() {
  const consent = useRecorderUi((s) => s.consent);
  const { accept, stopNow, dismiss } = useConsentActions();
  const before = consent?.mode === "before";
  const { data: settings } = useSettings();
  const cloud = settings ? settings.transcription.engine !== "whisper_local" : false;
  return (
    <DialogRoot open={consent !== null} onOpenChange={(open) => !open && dismiss()}>
      <DialogSurface>
        <div className="nt-consent-icon">
          <Icon icon={ShieldCheck} />
        </div>
        <DialogTitle>{before ? "Before you record" : "Margin is recording"}</DialogTitle>
        <DialogDescription asChild>
          <div className="nt-consent-body">
            <p>
              Recording other people may need their consent where you or they live. Tell everyone on
              the call that you are taking notes with Margin.
            </p>
            <p className="nt-faint">
              {cloud
                ? "Audio goes only to the transcription service you chose, and no bot joins the call."
                : "Audio stays on this Mac and no bot joins the call."}{" "}
              You will only see this once.
            </p>
          </div>
        </DialogDescription>
        <DialogActions>
          {before ? (
            <>
              <Button variant="secondary" onClick={dismiss}>
                Not now
              </Button>
              <Button variant="primary" icon={Mic} iconMotion="mic" onClick={accept}>
                Start recording
              </Button>
            </>
          ) : (
            <>
              <Button variant="secondary" onClick={stopNow}>
                Stop recording
              </Button>
              <Button variant="primary" onClick={accept}>
                Keep recording
              </Button>
            </>
          )}
        </DialogActions>
      </DialogSurface>
    </DialogRoot>
  );
}
