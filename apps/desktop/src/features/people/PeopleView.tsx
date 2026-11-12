/**
 * Saved people (spec › Speakers and Saved people): each with their colour, voice samples and
 * meetings, and rename, recolour, merge and delete. A person's colour shows on their avatar
 * ring in every transcript.
 */

import { Ellipsis, Merge, Palette, PenLine, Trash2 } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useState } from "react";
import { ShortcutTip } from "@/app/keybinds";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { CanvasBand } from "@/features/shell/CanvasBand";
import type { Person } from "@/ipc/types";
import { plural } from "@/lib/format";
import {
  Avatar,
  ConfirmDialog,
  Crumb,
  EmptyState,
  IconButton,
  ListItem,
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuSub,
  MenuTrigger,
  PageColumn,
  Popover,
  PopoverContent,
  PopoverTrigger,
  ScrollArea,
  SkeletonText,
  Swatch,
  ViewLayer,
} from "@/ui";
import { InlineName } from "../sidebar/InlineName";
import {
  useDeletePerson,
  useMergePeople,
  usePeople,
  useRenamePerson,
  useSetPersonColor,
} from "./hooks";

const COLORS = [1, 2, 3, 4, 5, 6, 7, 8] as const;
const COLOR_NAMES = ["Blue", "Teal", "Green", "Ochre", "Rust", "Rose", "Violet", "Slate"];

type Confirm =
  | { kind: "delete"; person: Person }
  | { kind: "merge"; source: Person; target: Person }
  | null;

export function PeopleView() {
  const reduced = useReducedMotion();
  const { data: people = [], isLoading } = usePeople();
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<Confirm>(null);
  const rename = useRenamePerson();
  const remove = useDeletePerson();
  const merge = useMergePeople();

  return (
    <ViewLayer>
      <CanvasBand start={<Crumb parts={["Saved people"]} />} />
      <ScrollArea>
        <PageColumn>
          <header className="nt-page-head">
            <div>
              <h1 className="t-title">Saved people</h1>
              <p className="t-callout">
                Margin recognises these voices and suggests them in new meetings. Voiceprints stay
                on this Mac.
              </p>
            </div>
          </header>
          {isLoading ? (
            <SkeletonText lines={4} />
          ) : people.length === 0 ? (
            <EmptyState
              art="people"
              title="No saved people yet"
              tip={
                <ShortcutTip action="note.togglePanel">
                  Flip between a note and its transcript
                </ShortcutTip>
              }
            >
              Name a speaker in a meeting transcript and choose Save as person. Margin then suggests
              them when it hears their voice again.
            </EmptyState>
          ) : (
            <div className="nt-list">
              <AnimatePresence initial={false}>
                {people.map((person) => (
                  <motion.div
                    key={person.id}
                    layout="position"
                    initial={false}
                    exit={{ opacity: 0, x: reduced ? 0 : 8, transition: tweens.exit }}
                    transition={reduced ? reducedTransition : springs.smooth}
                  >
                    <ListItem
                      className="nt-person-row"
                      tabIndex={0}
                      onKeyDown={(e) => {
                        if (e.key === "F2" || e.key === "Enter") {
                          e.preventDefault();
                          setRenaming(person.id);
                        }
                      }}
                      heading={
                        <span className="nt-person-name">
                          <Avatar name={person.name} color={person.color_index} size="lg" />
                          {renaming === person.id ? (
                            <span className="nt-person-rename">
                              <InlineName
                                initial={person.name}
                                placeholder="Name"
                                label="Person's name"
                                onCommit={(name) => {
                                  rename.mutate({ personId: person.id, name });
                                  setRenaming(null);
                                }}
                                onCancel={() => setRenaming(null)}
                              />
                            </span>
                          ) : (
                            <span className="nt-person-label">
                              <span>{person.name}</span>
                              <span className="nt-list-meta">
                                {plural(person.sample_count, "voice sample")} ·{" "}
                                {plural(person.meeting_count, "meeting")}
                              </span>
                            </span>
                          )}
                        </span>
                      }
                      actions={
                        <>
                          <Popover>
                            <PopoverTrigger asChild>
                              <IconButton icon={Palette} label="Change colour" />
                            </PopoverTrigger>
                            <PopoverContent align="end" label="Colour" className="nt-color-pop">
                              <div className="nt-popover-title">Colour for {person.name}</div>
                              <ColorPicker person={person} />
                            </PopoverContent>
                          </Popover>
                          <Menu>
                            <MenuTrigger asChild>
                              <IconButton icon={Ellipsis} label={`More for ${person.name}`} />
                            </MenuTrigger>
                            <MenuContent>
                              <MenuItem
                                icon={PenLine}
                                shortcut={["F2"]}
                                onSelect={() => setRenaming(person.id)}
                              >
                                Rename
                              </MenuItem>
                              {people.length > 1 && (
                                <MenuSub label="Merge into" icon={Merge}>
                                  {people
                                    .filter((p) => p.id !== person.id)
                                    .map((target) => (
                                      <MenuItem
                                        key={target.id}
                                        onSelect={() =>
                                          setConfirm({ kind: "merge", source: person, target })
                                        }
                                      >
                                        {target.name}
                                      </MenuItem>
                                    ))}
                                </MenuSub>
                              )}
                              <MenuSeparator />
                              <MenuItem
                                icon={Trash2}
                                iconMotion="trash"
                                danger
                                onSelect={() => setConfirm({ kind: "delete", person })}
                              >
                                Delete
                              </MenuItem>
                            </MenuContent>
                          </Menu>
                        </>
                      }
                    />
                  </motion.div>
                ))}
              </AnimatePresence>
            </div>
          )}
        </PageColumn>
      </ScrollArea>
      <ConfirmDialog
        open={confirm !== null}
        onOpenChange={(open) => !open && setConfirm(null)}
        destructive={confirm?.kind === "delete"}
        title={
          confirm?.kind === "delete"
            ? `Delete ${confirm.person.name}?`
            : confirm
              ? `Merge ${confirm.source.name} into ${confirm.target.name}?`
              : ""
        }
        body={
          confirm?.kind === "delete"
            ? "Margin forgets their voice and stops suggesting them. Past meetings keep the speaker's name. This cannot be undone."
            : confirm
              ? `${confirm.source.name}'s voice samples and meetings move to ${confirm.target.name}, and ${confirm.source.name} is removed. This cannot be undone.`
              : ""
        }
        confirmLabel={
          confirm?.kind === "delete"
            ? `Delete ${confirm.person.name}`
            : confirm
              ? `Merge into ${confirm.target.name}`
              : ""
        }
        onConfirm={() => {
          if (confirm?.kind === "delete") remove.mutate(confirm.person.id);
          else if (confirm)
            merge.mutate({ sourceId: confirm.source.id, targetId: confirm.target.id });
        }}
      />
    </ViewLayer>
  );
}

function ColorPicker({ person }: { person: Person }) {
  const setColor = useSetPersonColor();
  return (
    <div className="nt-swatches" role="radiogroup" aria-label="Speaker colour">
      {COLORS.map((n) => (
        <Swatch
          key={n}
          color={`var(--speaker-${n})`}
          label={COLOR_NAMES[n - 1] ?? `Colour ${n}`}
          selected={person.color_index === n}
          onSelect={() => setColor.mutate({ personId: person.id, color: n })}
        />
      ))}
    </div>
  );
}
