// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { describe, it, expect } from 'vitest';
import { isPlaylistLikeUrl } from '../../src/components/DownloadForm';

describe('isPlaylistLikeUrl (instant pre-classification, mirrors the Rust rule)', () => {
  it('treats a post under a handle as a single video', () => {
    for (const url of [
      'https://www.tiktok.com/@user/video/7581276453208149270',
      'https://www.tiktok.com/@user/video/7581276453208149270?is_from_webapp=1',
      'https://www.tiktok.com/@user/photo/7581276453208149270',
    ]) {
      expect(isPlaylistLikeUrl(url), url).toBe(false);
    }
  });

  it('keeps handles and their listing pages as playlists', () => {
    for (const url of [
      'https://www.tiktok.com/@user',
      'https://www.youtube.com/@handle',
      'https://www.youtube.com/@handle/videos',
      'https://www.youtube.com/@handle/shorts',
    ]) {
      expect(isPlaylistLikeUrl(url), url).toBe(true);
    }
  });
});
