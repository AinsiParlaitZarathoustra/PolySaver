// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React from 'react';
import { Box, Typography } from '@mui/material';
import type { InlineSpan, ReleaseNoteBlock } from '../utils/releaseNotes';

/** Renders text runs only: release texts never inject markup into the app. */
const Spans: React.FC<{ spans: InlineSpan[] }> = ({ spans }) => (
  <>
    {spans.map((span, i) =>
      span.bold ? (
        <strong key={i}>{span.text}</strong>
      ) : span.italic ? (
        <em key={i}>{span.text}</em>
      ) : (
        <React.Fragment key={i}>{span.text}</React.Fragment>
      ),
    )}
  </>
);

/** "What's new" blocks of a release: section headings, bullets, paragraphs. */
export const ReleaseNotes: React.FC<{ blocks: ReleaseNoteBlock[] }> = ({ blocks }) => {
  // Consecutive bullets share one list.
  const groups: ReleaseNoteBlock[][] = [];
  for (const block of blocks) {
    const previous = groups[groups.length - 1];
    if (block.kind === 'bullet' && previous?.[0].kind === 'bullet') previous.push(block);
    else groups.push([block]);
  }

  return (
    <Box sx={{ '& > *:first-of-type': { mt: 0 } }}>
      {groups.map((group, i) => {
        const first = group[0];
        if (first.kind === 'heading') {
          return (
            <Typography key={i} variant="subtitle1" sx={{ fontWeight: 700, mt: 2, mb: 0.5 }}>
              <Spans spans={first.spans} />
            </Typography>
          );
        }
        if (first.kind === 'paragraph') {
          return (
            <Typography key={i} variant="body2" sx={{ mb: 1 }}>
              <Spans spans={first.spans} />
            </Typography>
          );
        }
        return (
          <Box component="ul" key={i} sx={{ pl: 2.5, my: 0.5 }}>
            {group.map((bullet, j) => (
              <Typography component="li" variant="body2" key={j} sx={{ mb: 0.75 }}>
                <Spans spans={bullet.spans} />
              </Typography>
            ))}
          </Box>
        );
      })}
    </Box>
  );
};
