// The genres a wall can be narrowed to, as a list in a dialog: one is chosen, or "Any genre".
// Focus opens on the genre in force; the pad moves through the list like any group.

import { Lamp } from "@/components/Lamp";
import { Toggle } from "@/components/ui/toggle";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { useT } from "@/i18n";
import { Dialog } from "./Dialog";

const keyOf = (genre: string | null) => `genre:${genre ?? ""}`;

function Choice({ genre, chosen, onChoose }: { genre: string | null; chosen: boolean; onChoose: () => void }) {
  const t = useT();
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: keyOf(genre) });
  return (
    <Toggle
      ref={ref}
      {...props}
      variant="plain"
      pressed={chosen}
      onPressedChange={onChoose}
      className="relative justify-self-start focus-visible:bg-transparent focus:text-label"
    >
      {genre ?? t("genre.any")}
      {focused && <Lamp />}
    </Toggle>
  );
}

export interface GenreDialogProps {
  open: boolean;
  genres: string[];
  value: string | null;
  onChoose: (genre: string | null) => void;
  onBack: () => void;
}

export function GenreDialog({ open, genres, value, onChoose, onBack }: GenreDialogProps) {
  const t = useT();
  return (
    <Dialog
      id="genre"
      open={open}
      title={t("genre.title")}
      actions={[{ label: t("common.cancel"), onSelect: onBack }]}
      onBack={onBack}
      restoreKey={keyOf(value)}
    >
      <FocusGroup
        id="genre-list"
        orientation="spatial"
        aria-label={t("genre.list")}
        className="-mx-3 grid max-h-(--dialog-list) grid-cols-3 gap-x-2 gap-y-1 overflow-y-auto"
      >
        {[null, ...genres].map((g) => (
          <Choice key={keyOf(g)} genre={g} chosen={g === value} onChoose={() => onChoose(g)} />
        ))}
      </FocusGroup>
    </Dialog>
  );
}
