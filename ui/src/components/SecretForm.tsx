// Setting a key (TMDB's, the agent's): a secret text field, and buttons to paste one from the
// clipboard, save it and remove the one set. The value goes to the credential store and is
// cleared from here once saved; nothing ever reads it back.

import { useQueryClient } from "@tanstack/react-query";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { useState } from "react";
import { FocusGroup } from "@/focus/react";
import { useT } from "@/i18n";
import { plain, setSecret, type SecretName } from "@/lib/api";
import { keys } from "@/lib/queries";
import { FocusButton } from "./FocusButton";
import { TextField } from "./TextField";
import { toast } from "./ui/toast";

export interface SecretFormProps {
  name: SecretName;
  /** The field's placeholder and accessible name: "TMDB key". */
  label: string;
  /** Whether one is set now: Remove is offered only then. */
  isSet: boolean;
  /** Offer "Paste", reading the clipboard. */
  paste?: boolean;
  /** Offer "Remove". Default true. */
  removable?: boolean;
  /** The label of the save button. Default "Save". */
  saveLabel?: string;
  /** After the key is stored. */
  onSaved?: () => void;
  /** Focus keys start with this. Default `name`. */
  focusPrefix?: string;
}

export function SecretForm({
  name,
  label,
  isSet,
  paste = false,
  removable = true,
  saveLabel,
  onSaved,
  focusPrefix = name,
}: SecretFormProps) {
  const t = useT();
  const client = useQueryClient();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);

  const store = async (next: string | null) => {
    setBusy(true);
    try {
      await setSecret(name, next);
      setValue("");
      void client.invalidateQueries({ queryKey: keys.status });
      if (next !== null) onSaved?.();
    } catch (e) {
      toast(plain(e));
    } finally {
      setBusy(false);
    }
  };
  const save = () => {
    const v = value.trim();
    if (v !== "" && !busy) void store(v);
  };
  const fromClipboard = async () => {
    try {
      // Through the clipboard plugin: WebView2's own clipboard asks with a prompt the pad cannot answer.
      const text = (await readText()).trim();
      if (text === "") toast(t("secret.clipboardEmpty"));
      else setValue(text);
    } catch {
      toast(t("secret.clipboardFailed"));
    }
  };

  return (
    <FocusGroup id={`${focusPrefix}-form`} orientation="horizontal" aria-label={label} className="flex items-center gap-3">
      <TextField
        focusKey={`${focusPrefix}:field`}
        value={value}
        onChange={setValue}
        onSubmit={save}
        placeholder={label}
        secret
        className="w-(--key-field)"
      />
      {paste && (
        <FocusButton focusKey={`${focusPrefix}:paste`} onSelect={() => void fromClipboard()}>
          {t("secret.paste")}
        </FocusButton>
      )}
      <FocusButton focusKey={`${focusPrefix}:save`} onSelect={save} disabled={value.trim() === "" || busy}>
        {saveLabel ?? t("common.save")}
      </FocusButton>
      {removable && isSet && (
        <FocusButton focusKey={`${focusPrefix}:remove`} onSelect={() => void store(null)} disabled={busy} variant="plain">
          {t("common.remove")}
        </FocusButton>
      )}
    </FocusGroup>
  );
}
