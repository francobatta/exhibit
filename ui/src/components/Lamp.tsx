import { motion } from "motion/react";
import { focusSpring } from "@/lib/motion";

/**
 * The light on a focused label: a short lamp-white line under it that glides from item to item.
 * Every Lamp shares one `layoutId`, so the light travels between groups too. Render it inside the
 * item's box (`relative`); an item that clips its overflow would hide it otherwise.
 */
export function Lamp() {
  return (
    <motion.span
      layoutId="lamp"
      aria-hidden
      transition={focusSpring()}
      className="pointer-events-none absolute inset-x-0 bottom-1 mx-auto h-0.5 w-6 rounded-full bg-lamp"
    />
  );
}
