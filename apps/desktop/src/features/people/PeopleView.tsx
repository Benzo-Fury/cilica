/**
 * Saved people (spec › Speakers and Saved people): everyone Cilica can name in your meetings.
 * The list answers "who does Cilica know, and how well?" at a glance; opening someone (here,
 * or from a `[[link]]`, the graph or Connections) shows their page: the meetings they were in,
 * how well Cilica knows their voice, and ways to fix their name, colour or voice.
 */

import { Ellipsis, Eraser, X } from "lucide-react";
import { useState } from "react";
import { openPerson } from "@/app/router";
import { CanvasBand } from "@/features/shell/CanvasBand";
import type { Person } from "@/ipc/types";
import {
  ConfirmDialog,
  Crumb,
  EmptyState,
  IconButton,
  Menu,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuTrigger,
  PageColumn,
  PeopleList,
  PeopleNone,
  PersonRow,
  ScrollArea,
  SearchField,
  SkeletonText,
  ViewLayer,
  VoiceStatus,
  VoiceStrip,
} from "@/ui";
import { useForgetAllVoices, usePeople, usePersonVoice } from "./hooks";
import {
  forgetAllCopy,
  matchesName,
  metLine,
  sortPeople,
  VOICE_STATE_LABEL,
  voiceState,
} from "./model";
import { PersonPage } from "./PersonPage";

export function PeopleView({ personId }: { personId?: string }) {
  return personId ? <PersonPage personId={personId} /> : <PeopleIndex />;
}

function PeopleIndex() {
  const { data: people = [], isLoading } = usePeople();
  const [query, setQuery] = useState("");
  const [confirming, setConfirming] = useState(false);
  const forgetAll = useForgetAllVoices();
  const known = people.filter((p) => p.sample_count > 0).length;
  const shown = sortPeople(people).filter((p) => matchesName(p, query));
  const copy = forgetAllCopy(known);

  return (
    <ViewLayer>
      <CanvasBand start={<Crumb parts={["Saved people"]} />} />
      <ScrollArea>
        <PageColumn>
          <header className="nt-page-head">
            <div>
              <h1 className="t-title">Saved people</h1>
              <p className="t-callout">
                Cilica names who's speaking in your meeting transcripts by their voice. Pick who a
                speaker is once, and it learns their voice and names them on its own from then on.
                Voiceprints stay on this Mac.
              </p>
            </div>
            <Menu>
              <MenuTrigger asChild>
                <IconButton icon={Ellipsis} label="More for Saved people" />
              </MenuTrigger>
              <MenuContent align="end">
                {known > 0 ? (
                  <MenuItem icon={Eraser} danger onSelect={() => setConfirming(true)}>
                    Forget every voice…
                  </MenuItem>
                ) : (
                  <MenuLabel>Cilica doesn't know any voices yet.</MenuLabel>
                )}
              </MenuContent>
            </Menu>
          </header>
          {isLoading ? (
            <SkeletonText lines={4} />
          ) : people.length === 0 ? (
            <EmptyState art="people" title="No saved people yet">
              Name a speaker in a meeting transcript and choose Save as person. Cilica then names
              them when it hears their voice again.
            </EmptyState>
          ) : (
            <>
              {people.length > 3 && (
                <SearchField
                  keys={[]}
                  className="nt-people-find"
                  value={query}
                  placeholder="Find a person"
                  aria-label="Find a person"
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Escape" && query) {
                      e.stopPropagation();
                      setQuery("");
                    }
                    if (e.key === "Enter" && shown[0]) openPerson(shown[0].id);
                  }}
                  trailing={
                    query ? (
                      <IconButton icon={X} size="sm" label="Clear" onClick={() => setQuery("")} />
                    ) : (
                      <span className="nt-people-count">{people.length}</span>
                    )
                  }
                />
              )}
              <PeopleList label="Saved people">
                {shown.map((person, i) => (
                  <PersonListRow key={person.id} person={person} index={i} />
                ))}
              </PeopleList>
              {shown.length === 0 && <PeopleNone>No one called “{query.trim()}”.</PeopleNone>}
            </>
          )}
        </PageColumn>
      </ScrollArea>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        destructive
        title={copy.title}
        body={copy.body}
        confirmLabel="Forget every voice"
        onConfirm={() => forgetAll.mutate()}
      />
    </ViewLayer>
  );
}

function PersonListRow({ person, index }: { person: Person; index: number }) {
  const { data: voice } = usePersonVoice(person.id, person.sample_count > 0);
  const state = voiceState(person, voice);
  const shape = person.sample_count > 0 ? voice?.shape : null;
  return (
    <PersonRow
      color={person.color_index}
      name={person.name}
      meta={metLine(person)}
      index={index}
      onOpen={() => openPerson(person.id)}
      strip={
        <VoiceStrip
          color={person.color_index}
          rows={shape ? [shape.mean, ...shape.samples.slice(0, 3).map((s) => s.bins)] : []}
        />
      }
      status={<VoiceStatus tone={state}>{VOICE_STATE_LABEL[state]}</VoiceStatus>}
    />
  );
}
