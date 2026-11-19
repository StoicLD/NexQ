import { useEffect, useState } from "react";
import type { CodexModelOptions, ModelInfo } from "../lib/types";
import { getConfigFile } from "../stores/configStore";
import { showToast } from "../stores/toastStore";

interface Choice {
  value: string;
  label: string;
  disabled?: boolean;
}

// Each window edits the same per-model keys that the Rust provider reads at request time.
function PreferenceSelect({ label, settingKey, initial, choices }: {
  label: string;
  settingKey: string;
  initial: string;
  choices: Choice[];
}) {
  const [saved, setSaved] = useState<string>();
  const [ready, setReady] = useState(false);
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    let disposed = false;
    let changed = false;
    let unsubscribe: (() => void) | undefined;
    async function load() {
      const store = await getConfigFile();
      const off = await store.onKeyChange<string>(settingKey, (value) => {
        changed = true;
        if (!disposed) setSaved(value);
      });
      if (disposed) { off(); return; }
      unsubscribe = off;
      const value = await store.get<string>(settingKey);
      if (!disposed) {
        if (!changed) setSaved(value);
        setReady(true);
      }
    }
    load().catch((e) => { if (!disposed) showToast(String(e), "error"); });
    return () => { disposed = true; unsubscribe?.(); };
  }, [settingKey]);

  async function change(value: string) {
    setSaving(true);
    try {
      const store = await getConfigFile();
      await store.set(settingKey, value);
      await store.save();
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      setSaving(false);
    }
  }

  const value = saved ?? initial;
  return (
    <label className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
      {label}
      <select value={value} disabled={!ready || saving} onChange={(e) => void change(e.target.value)}
        className="rounded-lg border border-border/50 bg-background px-3 py-2 text-sm text-foreground focus:border-primary focus:outline-none disabled:opacity-50">
        {!choices.some((choice) => choice.value === value) && <option disabled value={value}>{value} (unavailable)</option>}
        {choices.map((choice) => <option key={choice.value} value={choice.value} disabled={choice.disabled}>{choice.label}</option>)}
      </select>
    </label>
  );
}

export function CodexModelSettings({ models, model }: { models: ModelInfo[]; model: string }) {
  const options = models.find((m) => m.id === model)?.codex;
  if (!options) return null;
  return <ModelPreferences key={options.model} options={options} />;
}

function ModelPreferences({ options }: { options: CodexModelOptions }) {
  const prefix = `codex.${options.model}`;
  return (
    <div className="mt-3 grid gap-3 sm:grid-cols-2">
      <PreferenceSelect label="Reasoning effort" settingKey={prefix + ".effort"} initial={options.initial_effort}
        choices={options.efforts.map((value) => ({ value, label: value }))} />
      <PreferenceSelect label="Speed" settingKey={prefix + ".speed"} initial="inherit" choices={[
        { value: "inherit", label: "Follow Codex default" },
        { value: "standard", label: "Standard" },
        { value: "fast", label: options.fast_tier ? "Fast (higher usage)" : "Fast (unavailable)", disabled: !options.fast_tier },
      ]} />
    </div>
  );
}
