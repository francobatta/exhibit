# Navigation and control hints

## Intent
Wherever the owner is, they can see how to go back and what the buttons do, with whatever they
are holding. Before this, Back worked only from the pad and keyboard (B, Esc, Backspace) and was
never shown except as a pad-only text line; a mouse could not leave Details, a show, Settings or
search at all, and clicking a section in the bar threw away the wall's place, sort and filters.

## Decisions (owner)
- Back only; no forward (option (a)). Back returns to where the owner was, as it already does.
- Hints follow the device last used: Xbox buttons for the pad, key caps for the keyboard, nothing
  for the mouse (it sees what it can click).

## Design
- **Back arrow.**
  - **Top bar:** on a screen stacked over another (Details, a show, Settings), the "Exhibit"
    mark gives way to "← Back" in the same slot (`--bar-lead`), so the sections never move.
    It is a bar item like the sections: pad, keys and mouse reach it.
  - **Search** covers the bar, so it carries "← Back" in the same place, its field below it.
    (An icon-only arrow in the gutter was tried first: cramped and faint.)
  - **First run:** shows "← Back" from its second step on, where the bar's would be.
  - The word is the hint's word for B ("Back", "Atrás"), so the bar and the hints agree.
- **Mouse back button** (XButton1) is Back everywhere, as B is. The forward button does nothing.
- **A section in the bar** that is already the root returns to it as it was, instead of opening
  it afresh.
- **Hint line** (bottom right, on a dark pill so it reads over artwork; a top-layer popover, raised
  as each dialog opens, since dialogs are top-layer popovers no z-index reaches over):
  - The actions are A, X Menu, Y Search (Ask once an agent exists), LB RB Sections and B Back.
    Each is shown only where it does something: Search only where Y works, Sections outside
    dialogs and first run. In a text field with the keyboard only Esc is shown: Enter, `/` and
    `[ ]` type there.
  - A names what it really does: a work's main action ("Play", "Resume", "Details" for a show),
    "Choose" for candidates and a wall in selection, else "Select".
  - Pad: filled round glyphs A B X Y, pills for LB/RB. Keys: key caps Enter, Esc, M, /, [ ]. The
    key names are in the catalogues (Spain says "Intro").
  - A modal with its own hint line (the on-screen keyboard) hides the global one.
- Monochrome glyphs (no accent colour), sizes in tokens.

## Approaches considered
- Back control in the screen content (a breadcrumb under the bar): it would need room on every
  screen and moves with the screen. Rejected for the bar slot.
- Icon-only arrow in the bar's gutter: no layout risk, but cramped beside the mark and wordless.
- Showing every device's hints at once: noisy; the owner chose the last device.

## Also fixed
- The wall's empty state and search's no-results lines went through the catalogues.
