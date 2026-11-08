// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

/**
 * Turns a GitHub release text into the "what's new" part shown in the update
 * dialog: everything before the download section, without any image (the
 * release page carries screenshots), as plain blocks rendered without HTML.
 */

/** Inline run of text, possibly bold or italic. */
export interface InlineSpan {
  text: string;
  bold?: boolean;
  italic?: boolean;
}

export type ReleaseNoteBlock =
  | { kind: 'heading'; spans: InlineSpan[] }
  | { kind: 'bullet'; spans: InlineSpan[] }
  | { kind: 'paragraph'; spans: InlineSpan[] };

export interface WhatsNew {
  /** Title line of the release text (e.g. "PolySaver 2.7.0: Playlists done right"). */
  title: string | null;
  blocks: ReleaseNoteBlock[];
}

const IMAGE_MARKDOWN = /!\[[^\]]*\]\([^)]*\)/g;
const IMAGE_HTML = /<img\b[^>]*>/gi;
const OTHER_HTML_TAG = /<\/?[a-z][^>]*>/gi;
const LINK = /\[([^\]]+)\]\([^)]*\)/g;

/**
 * The download guide and anything after it is not "what's new": it starts at
 * a horizontal rule, or at a heading that is just "Téléchargement" or
 * "Download" (emoji aside), so titles such as "smarter downloads" stay.
 */
function startsDownloadSection(line: string): boolean {
  if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) return true;
  const heading = /^\s*#{1,6}\s+(.*)$/.exec(line);
  if (!heading) return false;
  const words = heading[1].replace(/^[^\p{L}]+/u, '');
  return /^(t[ée]l[ée]chargements?|downloads?)\s*:?\s*$/i.test(words);
}

/** Splits `**bold**` and `*italic*` runs; links keep their text only. */
export function parseInline(raw: string): InlineSpan[] {
  const text = raw.replace(LINK, '$1');
  const spans: InlineSpan[] = [];
  const pattern = /\*\*([^*]+)\*\*|\*([^*]+)\*/g;
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (index > last) spans.push({ text: text.slice(last, index) });
    if (match[1] !== undefined) spans.push({ text: match[1], bold: true });
    else spans.push({ text: match[2], italic: true });
    last = index + match[0].length;
  }
  if (last < text.length) spans.push({ text: text.slice(last) });
  return spans.filter((span) => span.text.length > 0);
}

export function extractWhatsNew(body: string | null | undefined): WhatsNew {
  let title: string | null = null;
  const blocks: ReleaseNoteBlock[] = [];

  for (const rawLine of (body ?? '').split(/\r?\n/)) {
    if (startsDownloadSection(rawLine)) break;

    const line = rawLine
      .replace(IMAGE_MARKDOWN, '')
      .replace(IMAGE_HTML, '')
      .replace(OTHER_HTML_TAG, '')
      .trim();
    if (!line) continue;

    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const text = heading[2].trim();
      // The first top heading is the release title, shown as the dialog title.
      if (title === null && blocks.length === 0 && heading[1].length <= 2) {
        title = text;
      } else {
        blocks.push({ kind: 'heading', spans: parseInline(text) });
      }
      continue;
    }

    const bullet = /^[-*+]\s+(.*)$/.exec(line);
    if (bullet) {
      blocks.push({ kind: 'bullet', spans: parseInline(bullet[1]) });
      continue;
    }

    blocks.push({ kind: 'paragraph', spans: parseInline(line) });
  }

  return { title, blocks };
}
