/**
 * The Dynamic Island pill (DynamicIsland doc): compact 168 × 32 (dot, five-bar meter, timer) and
 * expanded 360 × 76 (title, status, Pause and Stop). Width, height and radius morph together with
 * `--spring-bouncy`; compact content blurs out as the full content scales in. True black on Macs
 * with a notch, dark glass elsewhere.
 */
import type { KeyboardEvent, ReactNode } from "react";

export interface IslandProps {
  expanded: boolean;
  glass: boolean;
  compact: ReactNode;
  full: ReactNode;
  onToggle: () => void;
  onPointerEnter?: () => void;
  onPointerLeave?: () => void;
  label: string;
}

export function Island({
  expanded,
  glass,
  compact,
  full,
  onToggle,
  onPointerEnter,
  onPointerLeave,
  label,
}: IslandProps) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onToggle();
    }
  };
  return (
    // biome-ignore lint/a11y/useSemanticElements: the pill holds buttons, so it cannot be a button itself
    <div
      role="button"
      tabIndex={0}
      aria-label={label}
      aria-expanded={expanded}
      className="nt-island nt-island-app"
      data-glass={glass ? "" : undefined}
      data-expanded={expanded ? "" : undefined}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest("button")) return;
        onToggle();
      }}
      onKeyDown={onKeyDown}
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
    >
      <div className="nt-island-compact" aria-hidden={expanded}>
        {compact}
      </div>
      <div className="nt-island-full" aria-hidden={!expanded} inert={!expanded}>
        {full}
      </div>
    </div>
  );
}

/** The round translucent button used inside the expanded island (Pause, Keep recording). */
export function IslandButton({
  label,
  children,
  onClick,
}: {
  label: string;
  children: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="nt-island-btn"
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/** The red Stop inside the expanded island. */
export function IslandStop({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      className="nt-stop nt-stop-lg"
      aria-label="Stop recording"
      title="Stop recording"
      onClick={onClick}
    />
  );
}
