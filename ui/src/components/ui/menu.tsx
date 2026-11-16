import {
  MenuContent as ContentPart,
  MenuGroupLabel as LabelPart,
  MenuItem as ItemPart,
  MenuSeparator as SeparatorPart,
} from "@ui-sdk/react/menu";
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";

export { Menu, MenuGroup, createMenuController } from "@ui-sdk/react/menu";

type Styled<T> = Omit<T, "className"> & { className?: string };

/** A raised panel on the wall, with a hairline edge; fades in and out. */
export function MenuContent({ className, ...props }: Styled<ComponentProps<typeof ContentPart>>) {
  return (
    <ContentPart
      className={cn(
        "z-50 flex w-menu flex-col rounded-control border border-line bg-wall-raised py-3 outline-none",
        "transition-opacity duration-(--dur-screen) data-starting-style:opacity-0 data-ending-style:opacity-0",
        "motion-reduce:transition-none",
        className,
      )}
      {...props}
    />
  );
}

/** A quiet heading over the items, such as the work's title. */
export function MenuLabel({ className, ...props }: Styled<ComponentProps<typeof LabelPart>>) {
  return (
    <LabelPart
      className={cn("truncate px-6 pt-1 pb-3 font-display text-caption text-label-faint", className)}
      {...props}
    />
  );
}

/**
 * An action: dim at rest, label white with a short lamp bar at its left edge when highlighted.
 * `hint` is a second line under the label, such as why the action cannot run now.
 */
export function MenuItem({
  className,
  children,
  hint,
  ...props
}: Omit<Styled<ComponentProps<typeof ItemPart>>, "children"> & { children: ReactNode; hint?: ReactNode }) {
  return (
    <ItemPart
      className={cn(
        "relative flex cursor-default flex-col gap-0.5 px-6 py-2.5 font-display text-lead font-ui text-label-dim outline-none",
        "data-highlighted:bg-lamp/10 data-highlighted:text-label focus:bg-lamp/10 focus:text-label",
        "before:absolute before:inset-y-2.5 before:left-0 before:w-0.5 before:rounded-full before:bg-lamp before:opacity-0",
        "data-highlighted:before:opacity-100 focus:before:opacity-100 data-disabled:text-label-faint",
        className,
      )}
      {...props}
    >
      <span>{children}</span>
      {hint !== undefined && <span className="font-text text-caption text-label-faint">{hint}</span>}
    </ItemPart>
  );
}

export function MenuSeparator({ className, ...props }: Styled<ComponentProps<typeof SeparatorPart>>) {
  return <SeparatorPart className={cn("mx-6 my-2 h-px bg-line", className)} {...props} />;
}
