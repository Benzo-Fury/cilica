/**
 * Settings › Swear jar: this week's jar, who filled it and with what, the weeks before, and what
 * counts. Reached from "Your data". The numbers wear the Home amber, like the faces.
 */
import { Play } from "lucide-react";
import { useState } from "react";
import { useEmptySwearJar, useSwearJar } from "@/features/swearjar/hooks";
import type { SwearJar } from "@/ipc/types";
import { money, plural } from "@/lib/format";
import { playSound } from "@/lib/sound";
import {
  Button,
  Chip,
  ConfirmDialog,
  GlanceBars,
  IconButton,
  JarHero,
  JarScope,
  JarWeeks,
  SegmentedControl,
  SettingsGroup,
  SettingsPage,
  SettingsRow,
  Switch,
} from "@/ui";
import { useSettings, useUpdateSettings } from "../hooks";

const PRICES = ["0", "50", "100", "200"] as const;
const DAY_MS = 86_400_000;

/** "14 Sep": the Monday a week started. */
const weekLabel = (iso: string) =>
  new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short" });

/** When the jar empties next, and what this week would have cost. */
function heroCaption(jar: SwearJar, priceCents: number, now = Date.now()): string {
  const days = Math.ceil((Date.parse(jar.week_start) + 7 * DAY_MS - now) / DAY_MS);
  const empties =
    days <= 1 ? "Empties tonight at midnight." : `Empties Monday at midnight, in ${days} days.`;
  if (priceCents === 0 || jar.count === 0) return empties;
  return `${money(priceCents * jar.count)} if this were real money. ${empties}`;
}

/** The record week, said plainly. */
function recordLine(jar: SwearJar): string | null {
  const record = jar.record;
  if (!record) return null;
  if (record.start === jar.week_start) return "This week is the jar's record so far.";
  return `The jar's record is ${record.count}, the week of ${weekLabel(record.start)}.`;
}

export function SwearJarPage() {
  const { data: settings } = useSettings();
  const { patch } = useUpdateSettings();
  const s = settings?.swear_jar;
  const { data: jar } = useSwearJar(true);
  const empty = useEmptySwearJar();
  const [confirm, setConfirm] = useState(false);
  if (!settings || !s) return null;
  const off = !s.enabled;
  const priceOptions = PRICES.map((p) => ({
    value: p,
    label: p === "0" ? "Off" : money(Number(p)),
  }));

  return (
    <SettingsPage
      title="Swear jar"
      description="Every swear said in a meeting or typed into a note puts a coin in. The jar empties every Monday."
    >
      {jar && (
        <JarScope>
          <JarHero
            count={jar.count}
            caption={heroCaption(jar, s.price_cents)}
            action={
              <Button size="sm" disabled={jar.count === 0} onClick={() => setConfirm(true)}>
                Empty the jar
              </Button>
            }
          />
          {jar.count > 0 && (
            <SettingsGroup title="This week">
              {jar.people.length > 0 && (
                <SettingsRow label="Who put coins in">
                  <GlanceBars
                    rows={jar.people.map((p, i) => ({
                      id: p.name ?? `you-${i}`,
                      label: p.name ?? "You",
                      value: p.count,
                      display: String(p.count),
                      color: p.name === null ? "accent" : (p.color_index ?? undefined),
                    }))}
                  />
                </SettingsRow>
              )}
              {jar.words.length > 0 && (
                <SettingsRow label="Favourite words">
                  <div className="nt-chip-row">
                    {jar.words.map((w) => (
                      <Chip key={w.word}>
                        {w.word} ×{w.count}
                      </Chip>
                    ))}
                  </div>
                </SettingsRow>
              )}
            </SettingsGroup>
          )}
          <SettingsGroup title="Past weeks" footer={recordLine(jar)}>
            <JarWeeks
              weeks={jar.weeks.map((w, i) => ({
                id: w.start,
                label: i === jar.weeks.length - 1 ? "This week" : weekLabel(w.start),
                count: w.count,
                current: i === jar.weeks.length - 1,
                record: jar.record?.start === w.start,
              }))}
            />
          </SettingsGroup>
        </JarScope>
      )}

      <SettingsGroup
        title="Counting"
        footer="Counted on this computer from a fixed list of common swears. Slurs aren't on it."
      >
        <SettingsRow
          id="jar-on"
          label="Swear jar"
          description="Count swears and show the jar. Off hides its faces on Home too."
          control={
            <Switch
              aria-labelledby="jar-on"
              checked={s.enabled}
              onCheckedChange={(enabled) => patch("swear_jar", { enabled })}
            />
          }
        />
        <SettingsRow
          id="jar-transcripts"
          label="Meeting transcripts"
          description="Everyone who speaks, as their words are transcribed. Counted even while Margin is in the background."
          control={
            <Switch
              aria-labelledby="jar-transcripts"
              disabled={off}
              checked={s.transcripts}
              onCheckedChange={(transcripts) => patch("swear_jar", { transcripts })}
            />
          }
        />
        <SettingsRow
          id="jar-notes"
          label="Notes you write"
          description="Counted when you finish the word. Pasted text and Margin's writing don't count, and deleting a word doesn't get the coin back."
          control={
            <Switch
              aria-labelledby="jar-notes"
              disabled={off}
              checked={s.notes}
              onCheckedChange={(notes) => patch("swear_jar", { notes })}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Showing">
        <SettingsRow
          id="jar-popup"
          label="Show the jar when a coin goes in"
          description="Off still counts, quietly."
          control={
            <Switch
              aria-labelledby="jar-popup"
              disabled={off}
              checked={s.popup}
              onCheckedChange={(popup) => patch("swear_jar", { popup })}
            />
          }
        />
        <SettingsRow
          label="Pretend price"
          description="What each swear would cost, for the small print on Home."
          control={
            <SegmentedControl
              label="Pretend price"
              options={priceOptions}
              value={String(s.price_cents) as (typeof PRICES)[number]}
              onChange={(v) => patch("swear_jar", { price_cents: Number(v) })}
            />
          }
        />
        <SettingsRow
          id="jar-clink"
          label="Clink"
          description={
            settings.sounds.enabled
              ? "A soft clink as a coin lands. Never while recording."
              : "Sounds are off in Settings › Sounds."
          }
          control={
            <>
              <IconButton
                icon={Play}
                label="Play the clink"
                tooltipSide="top"
                onClick={() => playSound("clink")}
              />
              <Switch
                aria-labelledby="jar-clink"
                disabled={off || !settings.sounds.enabled}
                checked={s.clink}
                onCheckedChange={(clink) => patch("swear_jar", { clink })}
              />
            </>
          }
        />
      </SettingsGroup>

      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        destructive
        title="Empty the jar?"
        body={`This week's ${plural(jar?.count ?? 0, "coin")} ${jar?.count === 1 ? "goes" : "go"}. Past weeks keep theirs. This cannot be undone.`}
        confirmLabel="Empty the jar"
        onConfirm={() => empty.mutate()}
      />
    </SettingsPage>
  );
}
