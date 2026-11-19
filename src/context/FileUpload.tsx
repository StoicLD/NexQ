import { useRef, useState } from "react";
import { CloudUpload, Loader2 } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { listContextFolder } from "../lib/ipc";
import { useContextStore } from "../stores/contextStore";
import { useConfigStore } from "../stores/configStore";
import { useRagStore } from "../stores/ragStore";

export function FileUpload() {
  const loadFile = useContextStore((s) => s.loadFile);
  const contextStrategy = useConfigStore((s) => s.contextStrategy);
  const autoIndexFile = useRagStore((s) => s.autoIndexFile);
  const [isDragOver, setIsDragOver] = useState(false);
  const importing = useRef(false);
  const indexQueue = useRef(Promise.resolve());
  const [progress, setProgress] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  async function importFiles(source: "files" | "folder" | string[]) {
    if (importing.current) return;
    importing.current = true;
    setErrorMessage(null);
    setNotice(null);
    try {
      let files: string[];
      let errors: string[] = [];
      if (source === "folder") {
        const selected = await open({ directory: true, title: "Add Context Folder" });
        if (!selected) return;
        setProgress("Scanning folder...");
        const folder = await listContextFolder(selected);
        files = folder.files;
        errors = folder.errors;
      } else if (source === "files") {
        const selected = await open({
          multiple: true,
          filters: [{ name: "Context Files", extensions: ["pdf", "txt", "md", "docx"] }],
        });
        if (!selected) return;
        files = selected;
      } else {
        files = source;
      }

      let imported = 0;
      for (const [index, path] of files.entries()) {
        setProgress(`Importing file ${index + 1} of ${files.length}...`);
        try {
          const resource = await loadFile(path);
          imported++;
          if (contextStrategy === "local_rag") {
            // Keep large folders from starting an embedding job for every file at once.
            indexQueue.current = indexQueue.current.then(() => autoIndexFile(resource.id));
          }
        } catch (e) {
          errors.push(`${path}: ${String(e)}`);
        }
      }
      if (source === "folder") {
        setNotice(files.length || errors.length
          ? `Imported ${imported} of ${files.length} files.`
          : "No supported documents found in this folder.");
      }
      if (errors.length) {
        setErrorMessage(errors.join("\n"));
      }
    } catch (e) {
      setErrorMessage(e instanceof Error ? e.message : String(e));
    } finally {
      setProgress(null);
      importing.current = false;
    }
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    setIsDragOver(false);
    const paths = Array.from(e.dataTransfer.files, (file) => (file as File & { path?: string }).path)
      .filter((path): path is string => !!path);
    if (paths.length) void importFiles(paths);
  }

  return (
    <div className="w-full">
      {/* Drop zone */}
      <div
        onDragOver={(e) => { e.preventDefault(); setIsDragOver(true); }}
        onDragLeave={() => setIsDragOver(false)}
        onDrop={handleDrop}
        aria-busy={!!progress}
        className={`relative flex flex-col items-center justify-center rounded-xl border-2 border-dashed transition-all duration-200 ${
          isDragOver
            ? "border-primary/60 bg-primary/5 scale-[1.01]"
            : "border-muted-foreground/20 bg-secondary/20 hover:border-muted-foreground/30 hover:bg-secondary/30"
        } px-6 py-6`}
      >
        {progress ? (
          <>
            <Loader2 className="mb-2 h-8 w-8 animate-spin text-primary/60" />
            <p className="text-sm font-medium text-muted-foreground">
              {progress}
            </p>
          </>
        ) : (
          <>
            <CloudUpload
              className={`mb-2 h-8 w-8 transition-colors duration-200 ${
                isDragOver ? "text-primary" : "text-primary/30"
              }`}
            />
            <p className="text-sm font-medium text-muted-foreground">
              {isDragOver ? "Drop to upload" : "Drag files here"}
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              PDF, TXT, Markdown, or DOCX
            </p>
            <div className="mt-3 flex gap-2">
              <button
                type="button"
                onClick={() => void importFiles("files")}
                className="rounded-lg border border-primary/20 bg-primary/5 px-4 py-1.5 text-xs font-medium text-primary transition-colors after:absolute after:inset-0 after:rounded-xl hover:bg-primary/10"
              >
                Browse Files
              </button>
              <button
                type="button"
                onClick={() => void importFiles("folder")}
                className="relative rounded-lg border border-primary/20 bg-primary/5 px-4 py-1.5 text-xs font-medium text-primary transition-colors hover:bg-primary/10"
              >
                Browse Folder
              </button>
            </div>
          </>
        )}
      </div>

      {notice && <p role="status" className="mt-2 text-xs text-muted-foreground">{notice}</p>}
      {errorMessage && (
        <div role="alert" className="mt-2 max-h-28 overflow-auto whitespace-pre-line break-words rounded-xl bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {errorMessage}
        </div>
      )}
    </div>
  );
}
