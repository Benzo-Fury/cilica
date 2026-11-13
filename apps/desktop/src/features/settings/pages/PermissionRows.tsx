/** The three OS permissions Cilica uses, as PermissionRows (Settings › Recording and onboarding). */
import { AppWindow, Mic, Volume2 } from "lucide-react";
import type { PermissionKind, Permissions } from "@/ipc/types";
import { PermissionRow } from "@/ui";
import { openPermissionSettings, useRequestPermission } from "../system";

const ROWS: { kind: PermissionKind; icon: typeof Mic; title: string; description: string }[] = [
  {
    kind: "microphone",
    icon: Mic,
    title: "Microphone",
    description: "Hears you in meetings and voice notes.",
  },
  {
    kind: "system_audio",
    icon: Volume2,
    title: "System audio",
    description:
      "Hears the other people on Zoom, Meet or Teams. No bot joins the call. macOS may only confirm it once you record.",
  },
  {
    kind: "accessibility",
    icon: AppWindow,
    title: "Accessibility",
    description: "Reads meeting window titles so recording can start on its own. Optional.",
  },
];

export function PermissionRows({
  permissions,
  include = ["microphone", "system_audio", "accessibility"],
}: {
  permissions: Permissions | undefined;
  include?: readonly PermissionKind[];
}) {
  const request = useRequestPermission();
  const rows = ROWS.filter((r) => include.includes(r.kind));
  // The first missing permission gets the one primary button.
  const next = rows.find(
    // Only the microphone is required; the optional rows never take the one primary button.
    (r) => r.kind === "microphone" && permissions?.[r.kind] === "not_determined",
  )?.kind;
  return (
    <div className="nt-perm-list">
      {rows.map((row) => (
        <PermissionRow
          key={row.kind}
          icon={row.icon}
          title={row.title}
          description={row.description}
          status={permissions?.[row.kind]}
          primary={row.kind === next}
          busy={request.isPending && request.variables === row.kind}
          unconfirmedNote={row.kind === "system_audio" ? "Not confirmed yet" : undefined}
          onRequest={() => request.mutate(row.kind)}
          onOpenSettings={() => openPermissionSettings(row.kind)}
        />
      ))}
    </div>
  );
}
