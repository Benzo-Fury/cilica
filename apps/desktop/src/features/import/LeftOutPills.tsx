/**
 * "Not imported": one soft red pill per kind of thing an import leaves behind. Hovering or
 * focusing a pill shows why it stays behind.
 */
import { X } from "lucide-react";
import { Chip, RichText, Tooltip } from "@/ui";

export interface LeftOutItem {
  kind: string;
  reason: string;
  count?: number;
}

export function LeftOutPills({ items }: { items: LeftOutItem[] }) {
  return (
    <div>
      <div className="nt-import-subtitle">Not imported</div>
      <ul className="nt-import-leftout">
        {items.map((item) => (
          <li key={item.kind}>
            <Tooltip
              side="top"
              delay={150}
              label={
                <span className="nt-tip-text">
                  <RichText text={item.reason} />
                </span>
              }
            >
              <Chip tone="danger" icon={X} tabIndex={0} className="nt-chip-hint">
                {item.kind}
                {item.count ? ` (${item.count})` : ""}
              </Chip>
            </Tooltip>
          </li>
        ))}
      </ul>
    </div>
  );
}
