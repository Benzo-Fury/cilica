/**
 * Renders Margin's Markdown replies as React elements (never HTML): paragraphs, headings, lists,
 * tasks, quotes, code and tables, with bold, italic, code, links and note citations inline.
 * While a reply streams (`streaming`), each word is its own span so new words fade in with
 * `nt-word-in` while earlier ones stay put.
 */
import { Fragment, type ReactNode, useMemo } from "react";
import { type Block, type Inline, parseInline, splitBlocks } from "@/lib/markdown";
import { cn } from "./cn";

export interface MarkdownProps {
  source: string;
  streaming?: boolean;
  /** Turn a citation ("Title", 12:34) into something clickable. */
  renderCitation?: (title: string, time: string | null, key: string) => ReactNode;
  /** External links open through this (the app decides how). */
  onLink?: (href: string) => void;
  className?: string;
}

export function Markdown({ source, streaming, renderCitation, onLink, className }: MarkdownProps) {
  const blocks = useMemo(() => splitBlocks(source), [source]);
  const ctx: Ctx = { streaming: Boolean(streaming), renderCitation, onLink };
  const groups = groupLists(blocks.filter((b) => b.kind !== "paragraph" || b.text.trim()));
  return (
    <div className={cn("nt-md", className)}>
      {groups.map((group, gi) =>
        Array.isArray(group) ? (
          renderList(group, `l${gi}`, ctx)
        ) : (
          // biome-ignore lint/suspicious/noArrayIndexKey: blocks are positional in a reply that only grows
          <Fragment key={`b${gi}`}>{renderBlock(group, `b${gi}`, ctx)}</Fragment>
        ),
      )}
    </div>
  );
}

interface Ctx {
  streaming: boolean;
  renderCitation?: MarkdownProps["renderCitation"];
  onLink?: MarkdownProps["onLink"];
}

/** Consecutive list items render as one list. */
function groupLists(blocks: Block[]): (Block | Block[])[] {
  const out: (Block | Block[])[] = [];
  for (const block of blocks) {
    const list = block.kind === "bullet" || block.kind === "ordered" || block.kind === "task";
    const prev = out[out.length - 1];
    if (list && Array.isArray(prev) && prev[0]?.kind === block.kind) prev.push(block);
    else out.push(list ? [block] : block);
  }
  return out;
}

function renderList(items: Block[], key: string, ctx: Ctx) {
  const first = items[0];
  const inner = items.map((item, i) => (
    <li
      // biome-ignore lint/suspicious/noArrayIndexKey: list items are positional in a reply
      key={i}
      className={cn(item.kind === "task" && "nt-md-task", item.checked && "nt-md-done")}
      style={item.depth ? { marginLeft: `${item.depth * 16}px` } : undefined}
    >
      {item.kind === "task" && (
        <span className="nt-md-box" role="img" aria-label={item.checked ? "Done" : "Open"} />
      )}
      <span>{inline(parseInline(item.text), `${key}-${i}`, ctx)}</span>
    </li>
  ));
  if (first?.kind === "ordered") {
    return (
      <ol key={key} start={first.number ?? 1}>
        {inner}
      </ol>
    );
  }
  return (
    <ul key={key} className={first?.kind === "task" ? "nt-md-tasks" : undefined}>
      {inner}
    </ul>
  );
}

function renderBlock(block: Block, key: string, ctx: Ctx): ReactNode {
  switch (block.kind) {
    case "heading": {
      const level = Math.min(4, Math.max(3, (block.level ?? 3) + 1));
      const Tag = level === 3 ? "h3" : "h4";
      return <Tag className="nt-md-h">{inline(parseInline(block.text), key, ctx)}</Tag>;
    }
    case "code":
      return (
        <pre className="nt-md-pre">
          <code>{block.text}</code>
        </pre>
      );
    case "quote":
      return <blockquote>{inline(parseInline(block.text), key, ctx)}</blockquote>;
    case "rule":
      return <hr />;
    case "table":
      return (
        <p className="nt-md-table">
          {block.text.split("\n").map((row, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: table rows are positional
            <span key={i}>{row}</span>
          ))}
        </p>
      );
    default:
      return <p>{inline(parseInline(block.text), key, ctx)}</p>;
  }
}

function words(text: string, key: string, streaming: boolean): ReactNode {
  if (!streaming) return text;
  const tokens = text.match(/\S+\s*|\s+/g) ?? [];
  return tokens.map((t, i) => (
    // biome-ignore lint/suspicious/noArrayIndexKey: words only ever append; the index is their identity
    <span key={`${key}w${i}`} className="nt-word-new">
      {t}
    </span>
  ));
}

function inline(nodes: Inline[], key: string, ctx: Ctx): ReactNode {
  return nodes.map((node, i) => {
    const k = `${key}.${i}`;
    switch (node.type) {
      case "text":
        return <Fragment key={k}>{words(node.text, k, ctx.streaming)}</Fragment>;
      case "strong":
        return <strong key={k}>{inline(node.children, k, ctx)}</strong>;
      case "em":
        return <em key={k}>{inline(node.children, k, ctx)}</em>;
      case "code":
        return <code key={k}>{node.text}</code>;
      case "link":
        return ctx.onLink && /^https?:\/\//.test(node.href) ? (
          <button
            key={k}
            type="button"
            className="nt-md-link"
            onClick={() => ctx.onLink?.(node.href)}
          >
            {node.text}
          </button>
        ) : (
          <span key={k} className="nt-md-link-text">
            {node.text}
          </span>
        );
      default:
        return ctx.renderCitation ? (
          <Fragment key={k}>{ctx.renderCitation(node.title, node.time, k)}</Fragment>
        ) : (
          <span key={k} className="nt-faint">
            ({`“${node.title}”`}
            {node.time ? `, ${node.time}` : ""})
          </span>
        );
    }
  });
}
