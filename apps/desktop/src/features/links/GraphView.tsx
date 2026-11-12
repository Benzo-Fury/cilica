/**
 * The graph (⌘G): every note and what it links to. Find is in the band; the few options sit
 * behind one button and start hidden. Click a note to open it, a note not created yet to create
 * it, a person to see Saved people. Drag to move, pinch or ⌘-scroll to zoom.
 */
import { Maximize, Minus, Plus, Search, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { navigate, openNote } from "@/app/router";
import { useCategories } from "@/features/notes/hooks";
import { CanvasBand } from "@/features/shell/CanvasBand";
import type { GraphNode } from "@/ipc/types";
import { plural } from "@/lib/format";
import {
  Crumb,
  EmptyState,
  Field,
  Graph,
  type GraphHandle,
  IconButton,
  Popover,
  PopoverContent,
  PopoverTrigger,
  SegmentedControl,
  Switch,
  ViewLayer,
} from "@/ui";
import { categoryColor, searchNodes, toView } from "./graphModel";
import { useCreateLinkedNote, useLinkGraph } from "./hooks";
import { useGraphOptions } from "./store";

const COLOR_BY = [
  { value: "type", label: "Type" },
  { value: "category", label: "Category" },
] as const;

export function GraphView({ focus }: { focus?: string }) {
  const { data: graph, isLoading } = useLinkGraph(null, 1);
  const { data: categories = [] } = useCategories();
  const options = useGraphOptions();
  const [query, setQuery] = useState("");
  const graphRef = useRef<GraphHandle>(null);
  const create = useCreateLinkedNote();

  const view = useMemo(
    () => (graph ? toView(graph, options, categories, focus ?? null) : { nodes: [], edges: [] }),
    [graph, options, categories, focus],
  );
  const matches = useMemo(() => searchNodes(view.nodes, query), [view.nodes, query]);
  const byKey = useMemo(
    () => new Map((graph?.nodes ?? []).map((n) => [n.key, n] as const)),
    [graph],
  );

  // Opened on a note or tag: bring it to the middle once the layout has had a moment.
  useEffect(() => {
    if (!focus || !view.nodes.some((n) => n.key === focus)) return;
    const t = setTimeout(() => graphRef.current?.centerOn(focus), 700);
    return () => clearTimeout(t);
  }, [focus, view.nodes]);

  const open = (key: string) => {
    const node: GraphNode | undefined = byKey.get(key);
    if (!node) return;
    switch (node.kind) {
      case "note":
        if (node.note_id) openNote(node.note_id);
        return;
      case "person":
        navigate({ name: "people" });
        return;
      case "missing":
        create.mutate({ title: node.label, categoryId: null });
        return;
      case "tag":
        setQuery(node.label);
        graphRef.current?.centerOn(key);
    }
  };

  const noteCount = graph?.nodes.filter((n) => n.kind === "note").length ?? 0;
  const linkCount = graph?.edges.filter((e) => e.kind !== "transcript").length ?? 0;

  return (
    <ViewLayer>
      <CanvasBand
        start={<Crumb parts={["Graph"]} />}
        center={
          <Field
            className="nt-graph-find"
            size="sm"
            icon={Search}
            type="search"
            placeholder="Find in graph"
            aria-label="Find in graph"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setQuery("");
              if (e.key === "Enter" && matches?.size) {
                const first = view.nodes.find((n) => matches.has(n.key));
                if (first) graphRef.current?.centerOn(first.key);
              }
            }}
            trailing={
              matches ? (
                <span className="nt-graph-find-count" aria-live="polite">
                  {matches.size === 0 ? "No match" : plural(matches.size, "match", "matches")}
                </span>
              ) : undefined
            }
          />
        }
        end={
          <Popover>
            <PopoverTrigger asChild>
              <IconButton icon={SlidersHorizontal} label="Graph options" />
            </PopoverTrigger>
            <PopoverContent align="end" label="Graph options" className="nt-graph-options">
              <div className="nt-menu-heading">Show</div>
              <OptionRow
                label="People"
                checked={options.people}
                onChange={(people) => options.set({ people })}
              />
              <OptionRow
                label="Tags"
                checked={options.tags}
                onChange={(tags) => options.set({ tags })}
              />
              <OptionRow
                label="Notes not created yet"
                checked={options.missing}
                onChange={(missing) => options.set({ missing })}
              />
              <div className="nt-menu-heading">Colour by</div>
              <SegmentedControl
                label="Colour by"
                options={COLOR_BY}
                value={options.colorBy}
                onChange={(colorBy) => options.set({ colorBy })}
                className="nt-graph-options-seg"
              />
            </PopoverContent>
          </Popover>
        }
      />
      <div className="nt-graph-stage">
        {!isLoading && noteCount === 0 ? (
          <EmptyState art="search" title="Nothing to show yet">
            Your notes and the links between them appear here. Type [[ in a note to link it to
            another.
          </EmptyState>
        ) : (
          <>
            <Graph
              ref={graphRef}
              layoutKey="global"
              nodes={view.nodes}
              edges={view.edges}
              focusKey={focus ?? null}
              highlight={matches}
              onOpen={open}
              label={`Graph of ${plural(noteCount, "note", "notes")} and ${plural(linkCount, "link", "links")}. Use Find in graph to look for a note.`}
            />
            {linkCount === 0 && !isLoading && (
              <p className="nt-graph-hint">
                No links yet. Type [[ in a note to link it to another.
              </p>
            )}
            <Legend colorBy={options.colorBy} categories={categories} filter={options} />
            <div className="nt-graph-zoom" role="toolbar" aria-label="Zoom">
              <IconButton
                icon={Minus}
                label="Zoom out"
                size="sm"
                onClick={() => graphRef.current?.zoomBy(1 / 1.4)}
              />
              <IconButton
                icon={Maximize}
                label="Fit to window"
                size="sm"
                onClick={() => graphRef.current?.fit()}
              />
              <IconButton
                icon={Plus}
                label="Zoom in"
                size="sm"
                onClick={() => graphRef.current?.zoomBy(1.4)}
              />
            </div>
          </>
        )}
      </div>
    </ViewLayer>
  );
}

function OptionRow({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: the switch is a button inside the label
    <label className="nt-graph-option">
      <span>{label}</span>
      <Switch checked={checked} onCheckedChange={onChange} label={label} />
    </label>
  );
}

/** What the dots and lines mean, plain in the corner. */
export function Legend({
  colorBy,
  categories,
  filter,
}: {
  colorBy: "type" | "category";
  categories: readonly { id: string; name: string }[];
  filter: { people: boolean; tags: boolean; missing: boolean };
}) {
  const items: { key: string; label: string; swatch: string; color?: number | null }[] = [];
  if (colorBy === "category" && categories.length) {
    for (const c of categories.slice(0, 7)) {
      items.push({
        key: c.id,
        label: c.name,
        swatch: "group",
        color: categoryColor(categories as never, c.id),
      });
    }
    items.push({ key: "none", label: "No category", swatch: "note" });
  } else {
    items.push({ key: "note", label: "Notes", swatch: "note" });
  }
  if (filter.people) items.push({ key: "person", label: "People", swatch: "person" });
  if (filter.tags) items.push({ key: "tag", label: "Tags", swatch: "tag" });
  if (filter.missing) items.push({ key: "missing", label: "Not created yet", swatch: "missing" });
  return (
    <ul className="nt-graph-legend" aria-label="Key">
      {items.map((item) => (
        <li key={item.key}>
          <span
            className="nt-graph-swatch"
            data-swatch={item.swatch}
            style={
              item.color
                ? ({ "--swatch": `var(--speaker-${item.color})` } as React.CSSProperties)
                : undefined
            }
            aria-hidden
          />
          {item.label}
        </li>
      ))}
      <li>
        <span className="nt-graph-swatch" data-swatch="line" aria-hidden />
        Link
      </li>
      {filter.people && (
        <li>
          <span className="nt-graph-swatch" data-swatch="dotted" aria-hidden />
          Heard in a meeting
        </li>
      )}
    </ul>
  );
}
