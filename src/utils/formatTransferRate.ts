// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import type { Language } from '../ipc/contracts';

/**
 * Formats a raw byte transfer rate (bytes per second) to localized MB/s (Mo/s in FR, MB/s in EN).
 * Conversion: MB/s = bytesPerSecond / 1_000_000.
 *
 * @param bytesPerSecond - Transfer rate in bytes/sec
 * @param language - Active application language ('fr' or 'en', defaults to 'fr')
 * @returns Formatted localized string, e.g. "2,50 Mo/s" or "2.50 MB/s" or empty string if absent/invalid
 */
export function formatTransferRate(
  bytesPerSecond?: number | null,
  language: Language = 'fr',
): string {
  if (
    bytesPerSecond === undefined ||
    bytesPerSecond === null ||
    !Number.isFinite(bytesPerSecond) ||
    bytesPerSecond <= 0
  ) {
    return '';
  }

  const megabytesPerSecond = bytesPerSecond / 1_000_000;
  const isEnglish = language === 'en';
  const locale = isEnglish ? 'en-GB' : 'fr-FR';
  const unit = isEnglish ? 'MB/s' : 'Mo/s';

  const formatted = megabytesPerSecond.toLocaleString(locale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });

  return `${formatted} ${unit}`;
}
