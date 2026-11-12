import { CalendarClock, Settings, Users, Waypoints } from "lucide-react";
import { useShortcut } from "@/app/keybinds";
import { navigate, openSettings, useRouter, type ViewName } from "@/app/router";
import { useTasks } from "@/features/tasks/hooks";
import { CountBadge, Row } from "@/ui";
import { SIDEBAR_PILL } from "./rows";

export function BottomSection() {
  const view = useRouter((s) => s.view);
  const current = (name: ViewName) => view.name === name;
  const { data: tasks } = useTasks();
  const pending = tasks?.reduce((sum, t) => sum + t.pending_count, 0) ?? 0;

  const graphKeys = useShortcut("graph.open");
  const go = (name: "tasks" | "people" | "graph") => () => navigate({ name });
  const activate = (fn: () => void) => (event: React.KeyboardEvent) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      fn();
    }
  };

  return (
    <div className="nt-sidebar-bottom">
      <Row
        role="button"
        tabIndex={0}
        icon={Waypoints}
        label="Graph"
        current={current("graph")}
        pillId={SIDEBAR_PILL}
        title={graphKeys.length ? `Graph (${graphKeys.join("")})` : "Graph"}
        onClick={go("graph")}
        onKeyDown={activate(go("graph"))}
      />
      <Row
        role="button"
        tabIndex={0}
        icon={CalendarClock}
        iconMotion="clock"
        label="Scheduled tasks"
        current={current("tasks")}
        pillId={SIDEBAR_PILL}
        trailing={
          pending > 0 ? (
            <CountBadge>
              {pending}
              <span className="nt-sr-only">{pending === 1 ? " proposal" : " proposals"}</span>
            </CountBadge>
          ) : undefined
        }
        onClick={go("tasks")}
        onKeyDown={activate(go("tasks"))}
      />
      <Row
        role="button"
        tabIndex={0}
        icon={Users}
        iconMotion="users"
        label="Saved people"
        current={current("people")}
        pillId={SIDEBAR_PILL}
        onClick={go("people")}
        onKeyDown={activate(go("people"))}
      />
      <Row
        role="button"
        tabIndex={0}
        icon={Settings}
        iconMotion="gear"
        label="Settings"
        current={current("settings")}
        pillId={SIDEBAR_PILL}
        onClick={() => openSettings(view.name === "settings" ? view.page : "general")}
        onKeyDown={activate(() => openSettings())}
      />
    </div>
  );
}
