/**
 * Dialogs (opaque `bg-raised`, 420px, over a scrim) and the glass overlays built on the same
 * Radix primitive (command palette, cheat sheet). Esc and the scrim cancel.
 */
import { Dialog as D } from "radix-ui";
import { type ReactNode, useRef } from "react";
import { Button } from "./Button";
import { cn } from "./cn";
import { useSquircleClip } from "./Squircle";

export const DialogRoot = D.Root;
export const DialogTrigger = D.Trigger;
export const DialogClose = D.Close;

export interface DialogSurfaceProps {
  children: ReactNode;
  /** `dialog`: opaque decision card. `glass`: strong glass sheet (palette, cheat sheet). */
  variant?: "dialog" | "glass";
  /** Accessible title when the visible one is not a `DialogTitle`. */
  title?: string;
  description?: string;
  className?: string;
  /** Where focus goes when it opens. */
  onOpenAutoFocus?: (event: Event) => void;
  /** Vertical placement: centred, or near the top like the palette. */
  placement?: "center" | "top";
}

/** The portal, scrim and surface. Put a `DialogTitle` inside or pass `title`. */
export function DialogSurface({
  children,
  variant = "dialog",
  title,
  description,
  className,
  onOpenAutoFocus,
  placement = "center",
}: DialogSurfaceProps) {
  const ref = useRef<HTMLDivElement>(null);
  useSquircleClip(ref, 20);
  return (
    <D.Portal>
      <D.Overlay className="nt-scrim nt-overlay-scrim" />
      <div className={cn("nt-overlay-frame", placement === "top" && "nt-overlay-frame-top")}>
        <D.Content
          ref={ref}
          onOpenAutoFocus={onOpenAutoFocus}
          aria-describedby={description ? undefined : undefined}
          className={cn(
            variant === "dialog" ? "nt-dialog" : "nt-palette nt-glass-strong",
            "nt-overlay-surface",
            className,
          )}
        >
          {title && <D.Title className="nt-sr-only">{title}</D.Title>}
          {description && <D.Description className="nt-sr-only">{description}</D.Description>}
          {children}
        </D.Content>
      </div>
    </D.Portal>
  );
}

export const DialogTitle = D.Title;
export const DialogDescription = D.Description;

export function DialogActions({ children }: { children: ReactNode }) {
  return <div className="nt-dialog-actions">{children}</div>;
}

export interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  body: ReactNode;
  /** Repeats the consequence: "Delete 3 notes". */
  confirmLabel: string;
  cancelLabel?: string;
  destructive?: boolean;
  onConfirm: () => void;
  confirmShortcut?: readonly string[];
}

/** A decision that cannot be undone. Focus starts on Cancel when destructive. */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  body,
  confirmLabel,
  cancelLabel = "Cancel",
  destructive = false,
  onConfirm,
  confirmShortcut,
}: ConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  return (
    <D.Root open={open} onOpenChange={onOpenChange}>
      <DialogSurface
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          (destructive ? cancelRef : confirmRef).current?.focus();
        }}
      >
        <D.Title>{title}</D.Title>
        <D.Description asChild>
          <p>{body}</p>
        </D.Description>
        <DialogActions>
          <D.Close asChild>
            <Button ref={cancelRef} variant="secondary">
              {cancelLabel}
            </Button>
          </D.Close>
          <Button
            ref={confirmRef}
            variant={destructive ? "danger" : "primary"}
            className={destructive ? "nt-btn-danger-solid" : undefined}
            shortcut={confirmShortcut}
            onClick={() => {
              onConfirm();
              onOpenChange(false);
            }}
          >
            {confirmLabel}
          </Button>
        </DialogActions>
      </DialogSurface>
    </D.Root>
  );
}
