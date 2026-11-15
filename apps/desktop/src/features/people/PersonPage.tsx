/**
 * One Saved person: their name and colour (changed in place), how well Cilica knows their
 * voice and why, every meeting they were in (each opens its transcript, and the ones Cilica
 * learnt their voice from can be taken out), and ways to merge, forget or delete them.
 */

import { Ellipsis, Eraser, Merge, PenLine, Trash2 } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useState } from "react";
import { reducedTransition, springs, tweens } from "@/app/motion";
import { openNote, openPerson } from "@/app/router";
import { CanvasBand } from "@/features/shell/CanvasBand";
import type { Person, PersonMeeting, PersonVoice } from "@/ipc/types";
import { plural } from "@/lib/format";
import {
  Button,
  ConfirmDialog,
  Crumb,
  DuplicateNotice,
  EmptyState,
  IconButton,
  ListMore,
  MeetingList,
  MeetingRow,
  MeetingsEmpty,
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuSub,
  MenuTrigger,
  NameField,
  PageColumn,
  PersonFoot,
  PersonHead,
  ScrollArea,
  SectionHead,
  SegmentedControl,
  SkeletonText,
  Swatch,
  ViewLayer,
  VoicePanel,
  VoiceSection,
  VoiceStat,
  VoiceStats,
  voiceColor,
} from "@/ui";
import {
  useDeletePerson,
  useForgetVoice,
  useMergePeople,
  usePeople,
  usePersonMeetings,
  usePersonVoice,
  useRemoveVoiceSample,
  useRenamePerson,
  useSetPersonColor,
} from "./hooks";
import {
  deleteCopy,
  firstName,
  forgetCopy,
  heardOn,
  isOdd,
  lastMet,
  likelyDuplicates,
  meetingLine,
  mergeCopy,
  removeSampleCopy,
  SHOWN_MEETINGS,
  samplesByMeeting,
  speechLength,
  talkShare,
  timeOfDay,
  VOICE_STATE_LABEL,
  voiceBody,
  voiceHeadline,
  voiceState,
} from "./model";

const COLORS = [1, 2, 3, 4, 5, 6, 7, 8] as const;
const COLOR_NAMES = ["Blue", "Teal", "Green", "Ochre", "Rust", "Rose", "Violet", "Slate"];

type Confirm =
  | { kind: "delete" }
  | { kind: "forget" }
  | { kind: "merge"; target: Person }
  | { kind: "removeSample"; sampleId: string; title: string; last: boolean }
  | null;

export function PersonPage({ personId }: { personId: string }) {
  const { data: people, isLoading } = usePeople();
  const person = people?.find((p) => p.id === personId);
  const { data: voice } = usePersonVoice(personId, !!person);
  const { data: meetings } = usePersonMeetings(personId, !!person);
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [renameKey, setRenameKey] = useState(0);
  const rename = useRenamePerson();
  const remove = useDeletePerson();
  const merge = useMergePeople();
  const forget = useForgetVoice();
  const removeSample = useRemoveVoiceSample();

  const band = (
    <CanvasBand
      start={<Crumb parts={["Saved people", person?.name ?? ""]} />}
      end={
        person && (
          <PersonMenu
            person={person}
            others={(people ?? []).filter((p) => p.id !== person.id)}
            onRename={() => setRenameKey((k) => k + 1)}
            onConfirm={setConfirm}
          />
        )
      }
    />
  );

  if (!person) {
    return (
      <ViewLayer>
        {band}
        <ScrollArea>
          <PageColumn>
            {isLoading ? (
              <SkeletonText lines={4} />
            ) : (
              <EmptyState
                art="people"
                title="This person isn't saved any more"
                actions={<Button onClick={() => openPerson()}>Back to Saved people</Button>}
              >
                They may have been deleted or merged into someone else.
              </EmptyState>
            )}
          </PageColumn>
        </ScrollArea>
      </ViewLayer>
    );
  }

  const dialog = confirmCopy(person, confirm);
  const duplicate = likelyDuplicates(person, people ?? [])[0];

  return (
    <ViewLayer>
      {band}
      <ScrollArea>
        <PageColumn className="nt-who" style={voiceColor(person.color_index)}>
          <PersonHead
            color={person.color_index}
            name={
              <NameField
                value={person.name}
                label="Name"
                focusKey={renameKey}
                onCommit={(name) => rename.mutate({ personId: person.id, name })}
              />
            }
            meta={headLine(person, meetings)}
            colour={<ColourChoice person={person} />}
          />
          <AnimatePresence initial={false}>
            {duplicate && (
              <motion.div
                key={duplicate.id}
                initial={{ opacity: 0, y: -4 }}
                animate={{ opacity: 1, y: 0, transition: springs.smooth }}
                exit={{ opacity: 0, transition: tweens.exit }}
              >
                <DuplicateNotice
                  action={
                    <Button
                      size="sm"
                      variant="secondary"
                      icon={Merge}
                      onClick={() => setConfirm({ kind: "merge", target: duplicate })}
                    >
                      Merge into {duplicate.name}
                    </Button>
                  }
                >
                  Is this the same person as <b>{duplicate.name}</b>? Merging puts their meetings
                  and voice together.
                </DuplicateNotice>
              </motion.div>
            )}
          </AnimatePresence>
          <Voice person={person} voice={voice} meetings={meetings} />
          <Meetings
            person={person}
            voice={voice}
            meetings={meetings}
            onRemoveSample={(sampleId, title, last) =>
              setConfirm({ kind: "removeSample", sampleId, title, last })
            }
          />
          <PersonFoot
            note={
              person.is_self
                ? "You are always here. Forgetting your voice keeps your meetings."
                : "Forgetting a voice keeps the person and their meetings."
            }
          >
            {person.sample_count > 0 && (
              <Button
                variant="ghost"
                size="sm"
                icon={Eraser}
                onClick={() => setConfirm({ kind: "forget" })}
              >
                Forget voice
              </Button>
            )}
            {!person.is_self && (
              <Button
                variant="danger"
                size="sm"
                icon={Trash2}
                iconMotion="trash"
                onClick={() => setConfirm({ kind: "delete" })}
              >
                Delete {firstName(person)}
              </Button>
            )}
          </PersonFoot>
        </PageColumn>
      </ScrollArea>
      <ConfirmDialog
        open={confirm !== null}
        onOpenChange={(open) => !open && setConfirm(null)}
        destructive={confirm?.kind !== "merge"}
        title={dialog.title}
        body={dialog.body}
        confirmLabel={dialog.confirm}
        onConfirm={() => {
          if (!confirm) return;
          switch (confirm.kind) {
            case "delete":
              openPerson();
              remove.mutate(person.id);
              break;
            case "forget":
              forget.mutate(person.id);
              break;
            case "merge":
              merge.mutate(
                { sourceId: person.id, targetId: confirm.target.id },
                { onSuccess: (target) => openPerson(target.id) },
              );
              break;
            case "removeSample":
              removeSample.mutate({ sampleId: confirm.sampleId, title: confirm.title });
              break;
          }
        }}
      />
    </ViewLayer>
  );
}

/** "9 meetings · last met today · 2 h 10 min of speech". */
function headLine(person: Person, meetings: PersonMeeting[] | undefined): string {
  if (person.is_self && person.meeting_count === 0) return "You, in every meeting you record";
  if (person.meeting_count === 0) return "Not in a meeting yet";
  const parts = [plural(person.meeting_count, "meeting")];
  if (person.last_met) parts.push(lastMet(person.last_met));
  const spoke = meetings?.reduce((a, m) => a + m.speech_ms, 0) ?? 0;
  if (spoke > 0) parts.push(`${speechLength(spoke)} of speech`);
  return parts.join(" · ");
}

function confirmCopy(
  person: Person,
  confirm: Confirm,
): { title: string; body: string; confirm: string } {
  if (!confirm) return { title: "", body: "", confirm: "" };
  switch (confirm.kind) {
    case "delete":
      return { ...deleteCopy(person), confirm: `Delete ${person.name}` };
    case "forget":
      return { ...forgetCopy(person), confirm: "Forget voice" };
    case "merge":
      return { ...mergeCopy(person, confirm.target), confirm: "Merge" };
    case "removeSample":
      return {
        ...removeSampleCopy(person, confirm.title, confirm.last),
        confirm: "Stop learning from it",
      };
  }
}

function PersonMenu({
  person,
  others,
  onRename,
  onConfirm,
}: {
  person: Person;
  others: Person[];
  onRename: () => void;
  onConfirm: (confirm: Confirm) => void;
}) {
  return (
    <Menu>
      <MenuTrigger asChild>
        <IconButton icon={Ellipsis} label={`More for ${person.name}`} />
      </MenuTrigger>
      <MenuContent align="end">
        <MenuItem icon={PenLine} onSelect={() => requestAnimationFrame(onRename)}>
          Rename
        </MenuItem>
        {!person.is_self && others.length > 0 && (
          <MenuSub label="Merge into" icon={Merge}>
            {others.map((target) => (
              <MenuItem key={target.id} onSelect={() => onConfirm({ kind: "merge", target })}>
                {target.name}
              </MenuItem>
            ))}
          </MenuSub>
        )}
        {person.sample_count > 0 && (
          <MenuItem icon={Eraser} onSelect={() => onConfirm({ kind: "forget" })}>
            Forget voice…
          </MenuItem>
        )}
        {!person.is_self && (
          <>
            <MenuSeparator />
            <MenuItem
              icon={Trash2}
              iconMotion="trash"
              danger
              onSelect={() => onConfirm({ kind: "delete" })}
            >
              Delete {person.name}…
            </MenuItem>
          </>
        )}
      </MenuContent>
    </Menu>
  );
}

function ColourChoice({ person }: { person: Person }) {
  const setColor = useSetPersonColor();
  return (
    <div className="nt-swatches" role="radiogroup" aria-label={`Colour for ${person.name}`}>
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

/** How well Cilica knows the voice, with the voiceprint once it has one. */
function Voice({
  person,
  voice,
  meetings,
}: {
  person: Person;
  voice: PersonVoice | undefined;
  meetings: PersonMeeting[] | undefined;
}) {
  const state = voiceState(person, voice);
  const shape = person.sample_count > 0 ? voice?.shape : null;
  const n = shape?.samples.length ?? 0;
  const whose = person.is_self ? "Your" : `${person.name}'s`;
  const latest = meetings?.[0];
  const corrected = voice?.corrected ?? 0;
  return (
    <VoiceSection
      tone={state}
      status={VOICE_STATE_LABEL[state]}
      headline={voiceHeadline(person, state)}
      body={voiceBody(person, state, voice)}
      action={
        state === "unknown" &&
        latest && (
          <Button
            size="sm"
            variant="secondary"
            onClick={() => openNote(latest.note_id, { panel: "transcript" })}
          >
            Open “{latest.title || "Untitled"}”
          </Button>
        )
      }
    >
      <VoicePanel
        bins={shape?.mean ?? EMPTY_VOICE}
        sweep
        label={
          shape
            ? `${whose} voiceprint, the average of ${plural(n, "meeting")}`
            : "No voiceprint yet"
        }
        caption={
          shape ? (
            <>
              Learnt from {plural(n, "meeting")}, {speechLength(voice?.speech_ms ?? 0)} of speech
            </>
          ) : (
            "Nothing learnt yet"
          )
        }
      />
      {shape && (
        <VoiceStats>
          <VoiceStat
            value={voice?.consistency == null ? "—" : `${Math.round(voice.consistency * 100)}%`}
            label={voice?.consistency == null ? "consistent, needs a second meeting" : "consistent"}
            meter={voice?.consistency ?? undefined}
          />
          <VoiceStat value={voice?.named ?? 0} label="named by Cilica" />
          <VoiceStat value={corrected} label={corrected === 1 ? "correction" : "corrections"} />
        </VoiceStats>
      )}
    </VoiceSection>
  );
}

/** A voiceprint with nothing in it, drawn faintly before Cilica knows the voice. */
const EMPTY_VOICE: number[] = Array.from({ length: 64 }, () => 0);

type MeetingFilter = "all" | "learnt";

function Meetings({
  person,
  voice,
  meetings,
  onRemoveSample,
}: {
  person: Person;
  voice: PersonVoice | undefined;
  meetings: PersonMeeting[] | undefined;
  onRemoveSample: (sampleId: string, title: string, last: boolean) => void;
}) {
  const reduced = useReducedMotion();
  const [all, setAll] = useState(false);
  const [filter, setFilter] = useState<MeetingFilter>("all");
  const samples = samplesByMeeting(person.sample_count > 0 ? voice : undefined);
  const count = voice?.shape?.samples.length ?? 0;
  const list = meetings ?? [];
  const learnt = list.filter((m) => samples.has(m.note_id)).length;
  const filtered = filter === "learnt" ? list.filter((m) => samples.has(m.note_id)) : list;
  const shown = all ? filtered : filtered.slice(0, SHOWN_MEETINGS);
  const who = person.is_self ? "Your meetings" : `Meetings with ${firstName(person)}`;
  const whose = person.is_self ? "your" : "their";
  return (
    <section className="nt-who-meetings" aria-label={who}>
      <SectionHead
        title={who}
        count={meetings ? list.length : undefined}
        end={
          learnt > 0 &&
          learnt < list.length && (
            <SegmentedControl<MeetingFilter>
              label="Show meetings"
              value={filter}
              onChange={(v) => {
                setFilter(v);
                setAll(false);
              }}
              options={[
                { value: "all", label: "All" },
                { value: "learnt", label: `Voice learnt from (${learnt})` },
              ]}
            />
          )
        }
      />
      {!meetings ? (
        <SkeletonText lines={3} />
      ) : list.length === 0 ? (
        <MeetingsEmpty>
          {person.is_self
            ? "Record a meeting and you'll show up here."
            : `When ${person.name} is named in a meeting transcript, the meeting shows up here.`}
        </MeetingsEmpty>
      ) : (
        <MeetingList>
          <AnimatePresence initial={false} mode="popLayout">
            {shown.map((m, i) => {
              const sample = samples.get(m.note_id);
              const odd = sample && isOdd(sample, count);
              const title = m.title.trim() || "Untitled";
              return (
                <motion.div
                  key={m.note_id}
                  layout="position"
                  initial={{ opacity: 0, y: reduced ? 0 : 4 }}
                  animate={{
                    opacity: 1,
                    y: 0,
                    transition: reduced ? reducedTransition : springs.smooth,
                  }}
                  exit={{ opacity: 0, transition: tweens.exit }}
                >
                  <MeetingRow
                    index={i}
                    day={heardOn(m.at)}
                    time={timeOfDay(m.at)}
                    title={title}
                    meta={meetingLine(m)}
                    share={talkShare(m)}
                    learnt={sample ? `Cilica learnt ${whose} voice from this meeting` : undefined}
                    odd={odd ? "Sounds unlike the others" : undefined}
                    tag={m.named_by === "cilica" ? "Named by Cilica" : undefined}
                    openLabel={`Open the transcript of ${title}`}
                    onOpen={() => openNote(m.note_id, { panel: "transcript" })}
                    actions={
                      sample && (
                        <IconButton
                          icon={Eraser}
                          size="sm"
                          label={`Stop learning ${whose} voice from ${title}`}
                          onClick={() => onRemoveSample(sample.id, title, count === 1)}
                        />
                      )
                    }
                  />
                </motion.div>
              );
            })}
          </AnimatePresence>
          {filtered.length > SHOWN_MEETINGS && (
            <ListMore onClick={() => setAll(!all)}>
              {all ? "Show fewer" : `Show all ${filtered.length}`}
            </ListMore>
          )}
        </MeetingList>
      )}
    </section>
  );
}
