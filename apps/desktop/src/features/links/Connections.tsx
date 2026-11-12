/**
 * Under a note: its local graph (what it links to and what links to it, one or two steps out),
 * then the notes that link here, and the notes that name it without a link, each with Link.
 */
import { ChevronRight, Expand, Link2 } from "lucide-react";
import { useMemo, useState } from "react";
import { navigate, openGraph, openNote } from "@/app/router";
import type { GraphNode, Mention } from "@/ipc/types";
import { displayTitle, plural } from "@/lib/format";
import { Button, Collapse, Graph, Icon, IconButton, SegmentedControl } from "@/ui";
import { toView } from "./graphModel";
import { useBacklinks, useCreateLinkedNote, useLinkGraph, useLinkMention } from "./hooks";

const DEPTHS = [
  { value: "1", label: "1 step" },
  { value: "2", label: "2 steps" },
] as const;

export function Connections({ noteId, categoryId }: { noteId: string; categoryId: string | null }) {
  const [depth, setDepth] = useState<"1" | "2">("1");
  const { data: graph } = useLinkGraph(noteId, Number(depth));
  const { data: backlinks } = useBacklinks(noteId);
  const [showUnlinked, setShowUnlinked] = useState(false);
  const link = useLinkMention();
  const create = useCreateLinkedNote();

  const view = useMemo(() => (graph ? toView(graph) : { nodes: [], edges: [] }), [graph]);
  const focusKey = graph && graph.focus !== null ? (graph.nodes[graph.focus]?.key ?? null) : null;
  const byKey = useMemo(
    () => new Map((graph?.nodes ?? []).map((n) => [n.key, n] as const)),
    [graph],
  );

  const linked = backlinks?.linked ?? [];
  const unlinked = backlinks?.unlinked ?? [];
  const connected = view.nodes.length > 1;
  if (!graph || !backlinks) return null;

  const open = (key: string) => {
    const node: GraphNode | undefined = byKey.get(key);
    if (!node || key === focusKey) return;
    if (node.kind === "note" && node.note_id) openNote(node.note_id);
    else if (node.kind === "person") navigate({ name: "people" });
    else if (node.kind === "missing") create.mutate({ title: node.label, categoryId });
    else if (node.kind === "tag") openGraph(key);
  };

  return (
    <section className="nt-connections" aria-label="Connections">
      <header className="nt-connections-head">
        <h2 className="nt-connections-title">Connections</h2>
        {connected && (
          <div className="nt-connections-tools">
            <SegmentedControl
              label="How far out"
              options={DEPTHS}
              value={depth}
              onChange={setDepth}
            />
            <IconButton
              icon={Expand}
              label="Open in graph"
              size="sm"
              onClick={() => openGraph(`n:${noteId}`)}
            />
          </div>
        )}
      </header>
      {connected ? (
        <Graph
          className="nt-connections-graph"
          variant="inline"
          layoutKey={`local:${noteId}`}
          nodes={view.nodes}
          edges={view.edges}
          focusKey={focusKey}
          onOpen={open}
          label={`This note's connections: ${plural(view.nodes.length - 1, "note, person or tag", "notes, people and tags")}.`}
        />
      ) : (
        <p className="nt-connections-empty">
          No links yet. Type [[ to link another note, or #tag to group notes.
        </p>
      )}

      {linked.length > 0 && (
        <div className="nt-mentions">
          <h3 className="nt-mentions-title">
            Linked mentions <span className="nt-mentions-count">{linked.length}</span>
          </h3>
          <ul>
            {linked.map((m) => (
              <MentionRow key={m.note.id} mention={m} />
            ))}
          </ul>
        </div>
      )}
      {unlinked.length > 0 && (
        <div className="nt-mentions">
          <button
            type="button"
            className="nt-mentions-toggle"
            aria-expanded={showUnlinked}
            onClick={() => setShowUnlinked((v) => !v)}
          >
            <Icon icon={ChevronRight} size="sm" className="nt-mentions-chevron" />
            Unlinked mentions <span className="nt-mentions-count">{unlinked.length}</span>
          </button>
          <Collapse open={showUnlinked}>
            <ul>
              {unlinked.map((m) => (
                <MentionRow
                  key={m.note.id}
                  mention={m}
                  action={
                    <Button
                      size="sm"
                      variant="secondary"
                      icon={Link2}
                      disabled={link.isPending}
                      onClick={() => link.mutate({ sourceId: m.note.id, targetId: noteId })}
                    >
                      Link
                    </Button>
                  }
                />
              ))}
            </ul>
          </Collapse>
        </div>
      )}
    </section>
  );
}

function MentionRow({ mention, action }: { mention: Mention; action?: React.ReactNode }) {
  return (
    <li className="nt-backlink">
      <button type="button" className="nt-backlink-open" onClick={() => openNote(mention.note.id)}>
        <span className="nt-backlink-title">{displayTitle(mention.note.title)}</span>
        {mention.excerpt && <span className="nt-backlink-excerpt">{mention.excerpt}</span>}
      </button>
      {action}
    </li>
  );
}
