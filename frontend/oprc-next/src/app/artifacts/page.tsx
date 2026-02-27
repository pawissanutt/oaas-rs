"use client";

import { useState, useEffect, useCallback } from "react";
import {
  Archive,
  Search,
  Download,
  Copy,
  FileCode,
  Loader2,
  RefreshCw,
  HardDrive,
  Clock,
  Package,
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { RawJsonDialog } from "@/components/ui/raw-json-dialog";
import { listArtifacts, getScriptSource, type ArtifactListEntry } from "@/lib/scripts-api";
import { toast } from "sonner";

export default function ArtifactsPage() {
  const [search, setSearch] = useState("");
  const [artifacts, setArtifacts] = useState<ArtifactListEntry[]>([]);
  const [loading, setLoading] = useState(true);

  // Source dialog state
  const [sourceDialogOpen, setSourceDialogOpen] = useState(false);
  const [sourceData, setSourceData] = useState<unknown>(null);
  const [sourceTitle, setSourceTitle] = useState("");
  const [loadingSource, setLoadingSource] = useState(false);

  const loadArtifacts = useCallback(async () => {
    setLoading(true);
    try {
      const data = await listArtifacts();
      setArtifacts(data);
    } catch (e) {
      console.error("Failed to load artifacts:", e);
      toast.error("Failed to load artifacts");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadArtifacts();
  }, [loadArtifacts]);

  const filtered = artifacts.filter((a) => {
    const q = search.toLowerCase();
    return (
      a.id.toLowerCase().includes(q) ||
      (a.source_package?.toLowerCase().includes(q) ?? false) ||
      (a.source_function?.toLowerCase().includes(q) ?? false)
    );
  });

  const handleCopyId = async (id: string) => {
    try {
      await navigator.clipboard.writeText(id);
      toast.success("Artifact ID copied");
    } catch {
      // Fallback for non-HTTPS / older browsers
      const textarea = document.createElement("textarea");
      textarea.value = id;
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand("copy");
      document.body.removeChild(textarea);
      toast.success("Artifact ID copied");
    }
  };

  const handleDownload = (url: string, id: string) => {
    const a = document.createElement("a");
    a.href = url;
    a.download = `${id.substring(0, 16)}.wasm`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  };

  const handleViewSource = async (pkg: string, fn: string) => {
    setLoadingSource(true);
    setSourceTitle(`Source: ${pkg}/${fn}`);
    try {
      const data = await getScriptSource(pkg, fn);
      setSourceData(data);
      setSourceDialogOpen(true);
    } catch (e) {
      toast.error(`Failed to fetch source: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoadingSource(false);
    }
  };

  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className="text-3xl font-bold tracking-tight">Artifacts</h1>
          <Badge variant="secondary">{artifacts.length}</Badge>
        </div>
        <Button variant="outline" onClick={loadArtifacts} disabled={loading}>
          <RefreshCw className={`mr-2 h-4 w-4 ${loading ? "animate-spin" : ""}`} />
          Refresh
        </Button>
      </div>

      <div className="flex w-full items-center space-x-2">
        <div className="relative flex-1 max-w-sm">
          <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input
            type="search"
            placeholder="Search artifacts by ID, package, or function..."
            className="pl-8"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      </div>

      <div className="space-y-3">
        {loading ? (
          <div className="text-center py-12 text-muted-foreground">
            <Loader2 className="h-6 w-6 animate-spin mx-auto mb-2" />
            Loading artifacts...
          </div>
        ) : filtered.length === 0 ? (
          <div className="text-center py-12 text-muted-foreground">
            {search ? "No artifacts match your search" : "No artifacts built yet. Go to Scripts to build one."}
          </div>
        ) : (
          filtered.map((artifact) => (
            <div
              key={artifact.id}
              className="flex items-center justify-between p-4 border border-border rounded-lg bg-card hover:bg-muted/30 transition-colors"
            >
              <div className="flex items-center gap-4 min-w-0">
                <div className="p-2 bg-amber-100 dark:bg-amber-900/30 rounded-full shrink-0">
                  <Archive className="h-5 w-5 text-amber-600 dark:text-amber-400" />
                </div>
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-sm truncate max-w-[300px]" title={artifact.id}>
                      {artifact.id.substring(0, 16)}...{artifact.id.substring(artifact.id.length - 8)}
                    </span>
                    <Badge variant="outline" className="shrink-0">
                      <HardDrive className="h-3 w-3 mr-1" />
                      {formatSize(artifact.size)}
                    </Badge>
                  </div>
                  {artifact.source_package && (
                    <p className="text-sm text-muted-foreground mt-1">
                      <Package className="h-3 w-3 inline mr-1" />
                      {artifact.source_package}/{artifact.source_function}
                    </p>
                  )}
                  {artifact.built_at && (
                    <p className="text-xs text-muted-foreground mt-0.5">
                      <Clock className="h-3 w-3 inline mr-1" />
                      {new Date(artifact.built_at).toLocaleString()}
                    </p>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-1 shrink-0">
                <Button
                  variant="ghost"
                  size="icon"
                  title="Copy artifact ID"
                  onClick={() => handleCopyId(artifact.id)}
                >
                  <Copy className="h-4 w-4" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  title="Download WASM artifact"
                  onClick={() => handleDownload(artifact.url, artifact.id)}
                >
                  <Download className="h-4 w-4" />
                </Button>
                {artifact.has_source && artifact.source_package && artifact.source_function && (
                  <Button
                    variant="ghost"
                    size="icon"
                    title="View source"
                    disabled={loadingSource}
                    onClick={() =>
                      handleViewSource(artifact.source_package!, artifact.source_function!)
                    }
                  >
                    <FileCode className="h-4 w-4" />
                  </Button>
                )}
              </div>
            </div>
          ))
        )}
      </div>

      {/* Source Dialog */}
      <RawJsonDialog
        open={sourceDialogOpen}
        onOpenChange={setSourceDialogOpen}
        title={sourceTitle}
        data={sourceData}
      />
    </div>
  );
}
