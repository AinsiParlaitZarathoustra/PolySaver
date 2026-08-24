// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import type { Language } from '../ipc/contracts';
import { en } from './locales/en';
import { fr } from './locales/fr';
import type { TranslationSchema } from './types';

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation';
    resources: {
      translation: TranslationSchema;
    };
  }
}

export const defaultLanguage: Language = 'fr';

export const resources = {
  fr: { translation: fr },
  en: { translation: en },
} as const;

i18n.use(initReactI18next).init({
  resources,
  lng: defaultLanguage,
  fallbackLng: defaultLanguage,
  interpolation: {
    escapeValue: false, // React already escapes values
  },
});

/**
 * Changes active language and dynamically updates the document's HTML lang attribute.
 */
export async function setAppLanguage(language: Language): Promise<void> {
  const normalizedLang: Language = language === 'en' ? 'en' : 'fr';
  if (i18n.language !== normalizedLang) {
    await i18n.changeLanguage(normalizedLang);
  }
  if (typeof document !== 'undefined' && document.documentElement) {
    document.documentElement.lang = normalizedLang;
  }
}

export default i18n;
