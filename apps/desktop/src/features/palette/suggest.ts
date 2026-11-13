/**
 * Filter suggestions for the word being typed: "sa" offers "With Sam Lee", "wo" offers "In Work",
 * "#pr" offers "#pricing". Picking one swaps the word for the filter's syntax.
 */
import { type FilterKind, type ParsedSearch, type SearchVocabulary, tokenize } from "./query";

export interface FilterSuggestion {
  key: string;
  kind: FilterKind;
  label: string;
  /** The whole input with the word replaced, ready for more typing. */
  input: string;
}

const MAX_SUGGESTIONS = 5;

function quote(text: string): string {
  return /\s/.test(text) ? `"${text}"` : text;
}

/** Whether any word of `name` starts with `prefix`. */
function startsWord(name: string, prefix: string): boolean {
  return name
    .toLowerCase()
    .split(/\s+/)
    .some((w) => w.startsWith(prefix));
}

export function suggestFilters(
  input: string,
  parsed: ParsedSearch,
  voc: SearchVocabulary,
): FilterSuggestion[] {
  // Only while a word is being typed: not after a space, not inside a filter.
  if (!input || /\s$/.test(input)) return [];
  const last = tokenize(input).at(-1);
  if (!last || last.key || last.quoted || last.link || last.negated) return [];
  const hash = last.value.startsWith("#");
  // A typed #tag is already a filter, but still gets completed.
  if (!hash && parsed.filters.some((f) => f.end === last.end)) return [];
  const prefix = last.word.replace(/^#/, "");
  if (prefix.length < (hash ? 1 : 2)) return [];

  const before = input.slice(0, last.start);
  const replace = (syntax: string) => `${before}${syntax} `;
  const out: FilterSuggestion[] = [];

  // Tags (always offered after a #, otherwise from two letters)
  for (const tag of voc.tags) {
    if (!tag.toLowerCase().startsWith(prefix) || tag.toLowerCase() === prefix) continue;
    out.push({ key: `tag:${tag}`, kind: "tag", label: `#${tag}`, input: replace(`#${tag}`) });
  }
  if (hash) return out.slice(0, MAX_SUGGESTIONS);

  // People
  for (const person of voc.people) {
    if (!startsWord(person.name, prefix) || parsed.query.person_ids?.includes(person.id)) continue;
    const label = person.is_self ? "With you" : `With ${person.name}`;
    out.push({
      key: `person:${person.id}`,
      kind: "person",
      label,
      input: replace(`with:${quote(person.name)}`),
    });
  }

  // Categories
  for (const category of voc.categories) {
    if (!startsWord(category.name, prefix) || parsed.query.category_ids?.includes(category.id))
      continue;
    out.push({
      key: `category:${category.id}`,
      kind: "category",
      label: `In ${category.name}`,
      input: replace(`in:${quote(category.name)}`),
    });
  }

  // Meeting apps
  if (!parsed.query.meeting_app) {
    for (const app of voc.meetingApps) {
      if (!startsWord(app, prefix)) continue;
      out.push({
        key: `app:${app}`,
        kind: "app",
        label: `On ${app}`,
        input: replace(`app:${quote(app)}`),
      });
    }
  }

  // Import sources, from three letters (short prefixes match too many apps)
  if (prefix.length >= 3 && !parsed.query.import_provider) {
    for (const provider of voc.importProviders) {
      if (!startsWord(provider.name, prefix)) continue;
      out.push({
        key: `source:${provider.id}`,
        kind: "source",
        label: `Imported from ${provider.name}`,
        input: replace(`from:${provider.id}`),
      });
    }
  }
  return out.slice(0, MAX_SUGGESTIONS);
}
