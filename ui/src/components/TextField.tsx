// A text field as a focus item. Reached with the arrows, it has native text focus at once, so a
// keyboard types into it and Enter submits. Selected with the pad (A), it opens the on-screen
// keyboard instead, which edits the same value; closing that returns focus to the field.

import { useState, type KeyboardEvent } from "react";
import { useFocusState } from "@/focus/bridge";
import { useFocusItem } from "@/focus/react";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { Keyboard } from "./Keyboard";
import { Input } from "./ui/input";

export interface TextFieldProps {
  /** Its focus key; the field must sit in a FocusGroup. */
  focusKey: string;
  value: string;
  onChange: (value: string) => void;
  /** Enter, or Done on the on-screen keyboard. */
  onSubmit?: (value: string) => void;
  /** Also its accessible name, and the on-screen keyboard's label. */
  placeholder: string;
  /** A key or password: dots, here and on the on-screen keyboard. */
  secret?: boolean;
  /** Digits only on a phone's keyboard, as a year. */
  inputMode?: "text" | "numeric";
  className?: string;
}

export function TextField({ focusKey, value, onChange, onSubmit, placeholder, secret = false, inputMode, className }: TextFieldProps) {
  const t = useT();
  const [typing, setTyping] = useState(false);
  const { ref, props } = useFocusItem<HTMLInputElement>({
    key: focusKey,
    hint: t("hints.type"),
    // A click only places the caret; the pad's A has no keys to type with, so it opens the keyboard.
    onSelect: () => {
      if (useFocusState.getState().mode === "pad") setTyping(true);
    },
  });
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && !e.nativeEvent.isComposing) {
      e.preventDefault();
      onSubmit?.(value);
      return;
    }
    props.onKeyDown(e);
  };
  return (
    <>
      <Input
        ref={ref}
        {...props}
        onKeyDown={onKeyDown}
        type={secret ? "password" : "text"}
        inputMode={inputMode}
        value={value}
        onValueChange={onChange}
        placeholder={placeholder}
        aria-label={placeholder}
        autoComplete="off"
        spellCheck={false}
        className={cn(className)}
      />
      <Keyboard
        open={typing}
        label={placeholder}
        value={value}
        onChange={onChange}
        secret={secret}
        onClose={() => setTyping(false)}
        onDone={() => {
          setTyping(false);
          onSubmit?.(value);
        }}
      />
    </>
  );
}
