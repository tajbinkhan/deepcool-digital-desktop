// Shared animation for drop-down menus: fades in while sliding and growing
// slightly from the top edge. Skipped when the system asks for reduced motion.

import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

const reduced = () => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

export function dropdown(_node: Element, { duration = 160 }: { duration?: number } = {}): TransitionConfig {
  if (reduced()) return { duration: 0 };
  return {
    duration,
    easing: cubicOut,
    css: (t) =>
      `opacity: ${t}; transform-origin: top; transform: translateY(${(1 - t) * -6}px) scaleY(${0.94 + 0.06 * t});`,
  };
}
