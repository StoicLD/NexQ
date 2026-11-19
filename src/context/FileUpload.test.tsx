import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { open } from "@tauri-apps/plugin-dialog";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listContextFolder } from "../lib/ipc";
import { useConfigStore } from "../stores/configStore";
import { useContextStore } from "../stores/contextStore";
import { useRagStore } from "../stores/ragStore";
import { FileUpload } from "./FileUpload";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../lib/ipc", () => ({ listContextFolder: vi.fn() }));

const loadFile = vi.fn();
const autoIndexFile = vi.fn();

beforeEach(() => {
  vi.resetAllMocks();
  useConfigStore.setState({ contextStrategy: "local_rag" });
  useContextStore.setState({ loadFile });
  useRagStore.setState({ autoIndexFile });
});

afterEach(cleanup);

it("imports the folder's documents through the file pipeline and continues after a bad file", async () => {
  vi.mocked(open).mockResolvedValue("/wiki");
  vi.mocked(listContextFolder).mockResolvedValue({
    files: ["/wiki/intro.md", "/wiki/bad.pdf", "/wiki/nested/intro.md"],
    errors: [],
  });
  loadFile.mockResolvedValueOnce({ id: "one" }).mockRejectedValueOnce("Invalid PDF")
    .mockResolvedValueOnce({ id: "two" });

  render(<FileUpload />);
  fireEvent.click(screen.getByRole("button", { name: "Browse Folder" }));
  expect(await screen.findByRole("status")).toHaveTextContent("Imported 2 of 3 files.");
  expect(screen.getByRole("alert")).toHaveTextContent("/wiki/bad.pdf");
  expect(open).toHaveBeenCalledTimes(1);
  expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true }));
  expect(listContextFolder).toHaveBeenCalledWith("/wiki");
  expect(loadFile.mock.calls.flat()).toEqual(["/wiki/intro.md", "/wiki/bad.pdf", "/wiki/nested/intro.md"]);
  expect(autoIndexFile.mock.calls.flat()).toEqual(["one", "two"]);
});

it("handles a cancelled folder dialog and still opens the regular file picker", async () => {
  vi.mocked(open).mockResolvedValueOnce(null).mockResolvedValueOnce(["/notes.txt"]);
  loadFile.mockResolvedValue({ id: "notes" });

  render(<FileUpload />);
  fireEvent.click(screen.getByRole("button", { name: "Browse Folder" }));
  await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
  fireEvent.click(screen.getByRole("button", { name: "Browse Files" }));
  await waitFor(() => expect(loadFile).toHaveBeenCalledWith("/notes.txt"));
  expect(listContextFolder).not.toHaveBeenCalled();
  expect(open).toHaveBeenCalledTimes(2);
});
