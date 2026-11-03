import { getLocale, setLocale } from "$lib/paraglide/runtime.js";

export type Language = "sv" | "en";

export const language = $state({
  current: "en" as Language
});

function updateDocumentLanguage(locale: Language) {
  document.documentElement.lang = locale;
}

export function initializeLanguage() {
  language.current = getLocale() as Language;
  updateDocumentLanguage(language.current);
}

export function changeLanguage(newLanguage: Language) {
  setLocale(newLanguage, { reload: false });
  language.current = newLanguage;
  updateDocumentLanguage(newLanguage);
}
