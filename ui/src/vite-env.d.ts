/// <reference types="vite/client" />

interface Window {
  /** Set by the shell before the page loads: whether the owner left the intro on. */
  __exhibitIntro?: boolean;
}

interface ImportMetaEnv {
  /** "1" opens the development gallery of work components. */
  readonly VITE_EXHIBIT_GALLERY?: string;
}
