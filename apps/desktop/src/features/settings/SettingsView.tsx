/** Settings (⌘,): a page list on the left, grouped rows on the right. Pages change with `gentle`. */
import { Activity, Trash2 } from "lucide-react";
import { AnimatePresence, LayoutGroup, motion, useReducedMotion } from "motion/react";
import { useEscapeLayer } from "@/app/layers";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { navigate, openSettings, type SettingsPageId, useRouter } from "@/app/router";
import { CanvasBand } from "@/features/shell/CanvasBand";
import { Crumb, Row, SkeletonText, ViewLayer } from "@/ui";
import { useSettings } from "./hooks";
import { ALL_SETTINGS_PAGES, DATA_PAGES, SETTINGS_PAGES, settingsPage } from "./pages";

/** Pages that used to sit in the sidebar and are reached from here now. */
const MORE = [
  { name: "activity", label: "Cilica activity", icon: Activity },
  { name: "trash", label: "Trash", icon: Trash2 },
] as const;

export function SettingsView({ page }: { page: SettingsPageId }) {
  const reduced = useReducedMotion();
  const back = useRouter((s) => s.back);
  const { data: settings } = useSettings();
  const def = settingsPage(page);
  const Page = def.component;
  const order = ALL_SETTINGS_PAGES.findIndex((p) => p.id === page);
  useEscapeLayer(true, back);

  return (
    <ViewLayer>
      <CanvasBand start={<Crumb parts={["Settings", def.label]} />} />
      <div className="nt-settings">
        <LayoutGroup id="settings-nav">
          <nav className="nt-settings-nav" aria-label="Settings pages">
            {SETTINGS_PAGES.map((p) => (
              <Row
                key={p.id}
                role="link"
                tabIndex={0}
                icon={p.icon}
                label={p.label}
                title={p.label}
                current={p.id === page}
                pillId="settings-pill"
                onClick={() => openSettings(p.id)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    openSettings(p.id);
                  }
                }}
              />
            ))}
            <div className="nt-settings-nav-label">Your data</div>
            {DATA_PAGES.map((p) => (
              <Row
                key={p.id}
                role="link"
                tabIndex={0}
                icon={p.icon}
                label={p.label}
                title={p.label}
                current={p.id === page}
                pillId="settings-pill"
                onClick={() => openSettings(p.id)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    openSettings(p.id);
                  }
                }}
              />
            ))}
            {MORE.map((item) => (
              <Row
                key={item.name}
                role="link"
                tabIndex={0}
                icon={item.icon}
                label={item.label}
                title={item.label}
                onClick={() => navigate({ name: item.name })}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    navigate({ name: item.name });
                  }
                }}
              />
            ))}
          </nav>
        </LayoutGroup>
        <div className="nt-settings-body">
          <AnimatePresence mode="popLayout" initial={false} custom={order}>
            <motion.div
              key={page}
              initial={{ opacity: 0, y: reduced ? 0 : 8 }}
              animate={{
                opacity: 1,
                y: 0,
                transition: reduced ? reducedTransition : springs.gentle,
              }}
              exit={{ opacity: 0, transition: tweens.exit }}
            >
              {settings ? (
                <Page />
              ) : (
                <div className="nt-settings-page">
                  <SkeletonText lines={6} />
                </div>
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </div>
    </ViewLayer>
  );
}
