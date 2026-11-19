import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ModelInfo } from "../lib/types";
import { CodexModelSettings } from "./CodexModelSettings";

const saved = new Map<string, string>();
const listeners = new Map<string, Set<(v: string) => void>>();
vi.mock("../stores/configStore", () => ({
  getConfigFile: async () => ({
    get: async (key: string) => saved.get(key),
    set: async (key: string, v: string) => {
      saved.set(key, v);
      listeners.get(key)?.forEach((fn) => fn(v));
    },
    save: async () => {},
    onKeyChange: async (key: string, fn: (v: string) => void) => {
      if (!listeners.has(key)) listeners.set(key, new Set());
      listeners.get(key)!.add(fn);
      return () => listeners.get(key)?.delete(fn);
    },
  }),
}));

const models: ModelInfo[] = ["default", "first", "second"].map((id) => ({
  id, name: id, provider: "codex_cli", context_window: null,
  codex: { model: id === "default" ? "first" : id, efforts: ["low", "high"], initial_effort: "low", fast_tier: id === "second" ? null : "priority" },
}));
beforeEach(() => { saved.clear(); listeners.clear(); });
afterEach(cleanup);

it("remembers choices on remount and shares the CLI default with the same named model", async () => {
  const view = render(<CodexModelSettings models={models} model="default" />);
  const speed = () => screen.getByLabelText("Speed");
  const effort = () => screen.getByLabelText("Reasoning effort");
  await waitFor(() => expect(speed()).toBeEnabled());
  fireEvent.change(speed(), { target: { value: "fast" } });
  fireEvent.change(effort(), { target: { value: "high" } });
  await waitFor(() => expect(saved.get("codex.first.effort")).toBe("high"));
  expect(saved.get("codex.first.speed")).toBe("fast");
  view.rerender(<CodexModelSettings models={models} model="second" />);
  await waitFor(() => expect(speed()).toBeEnabled());
  expect(speed()).toHaveValue("inherit");
  expect(screen.getByRole("option", { name: "Fast (unavailable)" })).toBeDisabled();
  view.unmount();
  render(<CodexModelSettings models={models} model="first" />);
  await waitFor(() => expect(speed()).toHaveValue("fast"));
  expect(effort()).toHaveValue("high");
});
