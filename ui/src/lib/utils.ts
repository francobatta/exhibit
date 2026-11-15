import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

// Our type scale replaces Tailwind's, so tailwind-merge must know `text-title` is a size, not a colour.
const merge = extendTailwindMerge({
  extend: {
    theme: { text: ["caption", "body", "lead", "heading", "title", "display", "hero"] },
  },
});

export function cn(...inputs: ClassValue[]): string {
  return merge(clsx(inputs));
}
