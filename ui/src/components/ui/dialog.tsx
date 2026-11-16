import {
  DialogBackdrop as BackdropPart,
  DialogClose as ClosePart,
  DialogContent as ContentPart,
  DialogDescription as DescriptionPart,
  DialogTitle as TitlePart,
} from "@ui-sdk/react/dialog";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export { Dialog, DialogTrigger, createDialogController } from "@ui-sdk/react/dialog";

/** The dimmed wall behind a dialog. */
export const dialogBackdrop = cn(
  "fixed inset-0 z-40 bg-wall/80 transition-opacity duration-(--dur-screen)",
  "data-starting-style:opacity-0 data-ending-style:opacity-0",
);

/** The dialog panel: raised wall, hairline, centred; enters and leaves with a short fade. */
export const dialogPanel = cn(
  "fixed top-1/2 left-1/2 z-50 flex w-[min(40rem,calc(100vw-4rem))] -translate-1/2 flex-col gap-6",
  "rounded-control border border-line bg-wall-raised p-8 text-label outline-none",
  "transition-[opacity,scale] duration-(--dur-screen)",
  "data-starting-style:scale-[0.98] data-starting-style:opacity-0",
  "data-ending-style:scale-[0.98] data-ending-style:opacity-0",
  "motion-reduce:transition-opacity",
);

type Styled<T> = Omit<T, "className"> & { className?: string };

/** The panel and the dimmed wall behind it. Enter and exit use UI SDK's starting/ending styles. */
export function DialogContent({
  className,
  children,
  ...props
}: Styled<ComponentProps<typeof ContentPart>>) {
  return (
    <>
      <BackdropPart className={dialogBackdrop} />
      <ContentPart
        className={cn(dialogPanel, className)}
        {...props}
      >
        {children}
      </ContentPart>
    </>
  );
}

export function DialogTitle({ className, ...props }: Styled<ComponentProps<typeof TitlePart>>) {
  return (
    <TitlePart
      className={cn("font-display text-heading font-title tracking-title text-label", className)}
      {...props}
    />
  );
}

export function DialogDescription({
  className,
  ...props
}: Styled<ComponentProps<typeof DescriptionPart>>) {
  return (
    <DescriptionPart
      className={cn("font-reading text-lead leading-reading text-label-dim", className)}
      {...props}
    />
  );
}

/** A Close part; style it like a Button with `buttonVariants`. */
export function DialogClose({ className, ...props }: Styled<ComponentProps<typeof ClosePart>>) {
  return <ClosePart className={className} {...props} />;
}
