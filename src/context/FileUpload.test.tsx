import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listContextFolder, removeContextFiles } from "../lib/ipc";
import { useConfigStore } from "../stores/configStore";
import { useContextStore } from "../stores/contextStore";
import { useRagStore } from "../stores/ragStore";
import { FileUpload } from "./FileUpload";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), confirm: vi.fn() }));
vi.mock("../lib/ipc", () => ({ listContextFolder: vi.fn(), removeContextFiles: vi.fn() }));

const loadFile = vi.fn();
const autoIndexFile = vi.fn();

beforeEach(() => {
  vi.resetAllMocks();
  useConfigStore.setState({ contextStrategy: "local_rag" });
  useContextStore.setState({ loadFile, resources: [], loadResources: vi.fn() });
  useRagStore.setState({ autoIndexFile, resetTestSearch: vi.fn(), refreshIndexStatus: vi.fn() });
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

function showUploadedSource() {
  useContextStore.setState({ resources: [{
    id: "one", name: "wiki.md", file_type: "md", file_path: "/context/one_wiki.md",
    size_bytes: 40, token_count: 10, preview: "wiki", loaded_at: "",
  }] });
  render(<FileUpload />);
  fireEvent.click(screen.getByRole("button", { name: "Clear all sources" }));
}

it.each([{ decisions: [false] }, { decisions: [true, false] }])("cancels without deleting for $decisions", async ({ decisions }) => {
  for (const decision of decisions) vi.mocked(confirm).mockResolvedValueOnce(decision);
  showUploadedSource();
  await waitFor(() => expect(confirm).toHaveBeenCalledTimes(decisions.length));
  expect(removeContextFiles).not.toHaveBeenCalled();
  expect(useContextStore.getState().resources).toHaveLength(1);
});

it("removes only the approved sources after two confirmations", async () => {
  vi.mocked(confirm).mockResolvedValueOnce(true).mockImplementationOnce(async () => {
    expect(removeContextFiles).not.toHaveBeenCalled();
    const [first] = useContextStore.getState().resources;
    useContextStore.getState().addResource({ ...first, id: "later" });
    return true;
  });
  showUploadedSource();
  expect(await screen.findByRole("status")).toHaveTextContent("Removed 1 uploaded file.");
  expect(vi.mocked(removeContextFiles).mock.calls).toEqual([[["one"]]]);
  expect(useContextStore.getState().loadResources).toHaveBeenCalledOnce();
  expect(useRagStore.getState().refreshIndexStatus).toHaveBeenCalledOnce();
});
