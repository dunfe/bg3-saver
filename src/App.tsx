import { useState, useEffect, useCallback, useMemo, memo } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Folder,
  Archive,
  Shield,
  Trash2,
  ArrowRightCircle,
  RefreshCcw,
  Gamepad2,
  Clock,
  ExternalLink,
  History,
  AlertCircle
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle, CardContent, CardDescription, CardFooter } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Toaster } from "@/components/ui/sonner";
import { toast } from "sonner";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";

interface SaveFolder {
  name: string;
  path: string;
  last_modified: number;
}

const SavePreview = memo(({ path, last_modified, onExpand, className = "w-32 h-[72px]" }: { path: string; last_modified: number; onExpand: (url: string) => void, className?: string }) => {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let currentUrl: string | null = null;

    invoke<number[]>("get_save_preview", { path })
      .then((bytes) => {
        if (!active) return;
        const ui8 = new Uint8Array(bytes);
        const blob = new Blob([ui8], { type: "image/webp" });
        currentUrl = URL.createObjectURL(blob);
        setUrl(currentUrl);
      })
      .catch(() => { });

    return () => {
      active = false;
      if (currentUrl) URL.revokeObjectURL(currentUrl);
    }
  }, [path, last_modified]);

  if (!url) {
    return (
      <div className={`${className} bg-zinc-800/40 rounded-lg flex items-center justify-center border border-white/5 shrink-0 shimmer`}>
        <span className="text-[10px] text-zinc-500 font-bold tracking-tighter">PREVIEW</span>
      </div>
    );
  }

  return (
    <div className={`${className} relative group overflow-hidden rounded-lg border border-white/10 shrink-0 shadow-lg cursor-pointer`}>
      <img
        src={url}
        alt="Save preview"
        onClick={() => onExpand(url)}
        className="w-full h-full object-cover transition-transform duration-500 group-hover:scale-110"
      />
      <div className="absolute inset-0 bg-black/20 group-hover:bg-black/0 transition-colors pointer-events-none" />
    </div>
  );
});

SavePreview.displayName = "SavePreview";

function App() {
  const [saves, setSaves] = useState<SaveFolder[]>([]);
  const [backups, setBackups] = useState<SaveFolder[]>([]);
  const [loading, setLoading] = useState(false);
  const [fullscreenImage, setFullscreenImage] = useState<string | null>(null);

  // Dialog states
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [confirmRestore, setConfirmRestore] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      const activeSaves: SaveFolder[] = await invoke("get_saves");
      const savedBackups: SaveFolder[] = await invoke("get_backups");

      setSaves(prev => JSON.stringify(prev) === JSON.stringify(activeSaves) ? prev : activeSaves);
      setBackups(prev => JSON.stringify(prev) === JSON.stringify(savedBackups) ? prev : savedBackups);
    } catch (e) {
      console.error(e);
      toast.error("Failed to load save data", {
        description: String(e)
      });
    }
  }, []);

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 10000);
    return () => clearInterval(interval);
  }, []);

  const handleBackup = useCallback(async (name: string) => {
    setLoading(true);
    const promise = invoke("backup_save", { saveName: name });

    toast.promise(promise, {
      loading: `Backing up ${name}...`,
      success: () => {
        loadData();
        return `Successfully backed up ${name}`;
      },
      error: (e) => `Failed to backup: ${e}`,
    });

    try {
      await promise;
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [loadData]);

  const handleRestore = useCallback(async (name: string) => {
    setLoading(true);
    try {
      await invoke("restore_backup", { backupName: name });
      await loadData();
      toast.success("Backup Restored", {
        description: `Your active save has been replaced with ${name}.`
      });
    } catch (e) {
      console.error(e);
      toast.error("Restore Failed", {
        description: String(e)
      });
    } finally {
      setLoading(false);
      setConfirmRestore(null);
    }
  }, [loadData]);

  const handleDelete = useCallback(async (name: string) => {
    setLoading(true);
    try {
      await invoke("delete_backup", { backupName: name });
      await loadData();
      toast.success("Backup Deleted", {
        description: `${name} has been removed from your backups.`
      });
    } catch (e) {
      console.error(e);
      toast.error("Delete Failed", {
        description: String(e)
      });
    } finally {
      setLoading(false);
      setConfirmDelete(null);
    }
  }, [loadData]);

  const formatDate = (ts: number) => {
    return new Date(ts * 1000).toLocaleString();
  };

  const latestSave = useMemo(() => 
    saves.length > 0 ? [...saves].sort((a, b) => b.last_modified - a.last_modified)[0] : null
  , [saves]);


  return (
    <TooltipProvider>
      <div className="dark h-full bg-zinc-950 text-slate-50 flex flex-col font-sans selection:bg-indigo-500/30">
        <Toaster closeButton position="top-right" theme="dark" richColors />

        {/* Animated Background */}
        <div className="fixed inset-0 pointer-events-none overflow-hidden">
          <div className="absolute top-[-10%] left-[-10%] w-[40%] h-[40%] rounded-full bg-indigo-600/10 blur-[120px] animate-pulse" />
          <div className="absolute bottom-[-10%] right-[-10%] w-[40%] h-[40%] rounded-full bg-purple-600/10 blur-[120px] animate-pulse [animation-delay:2s]" />
          <div className="absolute top-[20%] right-[10%] w-[20%] h-[20%] rounded-full bg-indigo-400/5 blur-[80px]" />
        </div>

        <header className="relative z-20 flex justify-between items-center px-8 py-6 border-b border-white/5 glass">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-indigo-500/10 rounded-xl border border-indigo-500/20 shadow-[0_0_15px_-3px_rgba(99,102,241,0.4)]">
              <Gamepad2 className="w-6 h-6 text-indigo-400" />
            </div>
            <div>
              <h1 className="text-xl font-bold tracking-tight text-gradient">
                BG3 Saver
              </h1>
              <p className="text-[10px] text-zinc-500 font-semibold tracking-widest uppercase">Save Manager Pro</p>
            </div>
          </div>

          <div className="flex items-center gap-6">
            <div className="flex items-center gap-3 bg-zinc-900/40 border border-white/5 py-1.5 px-4 rounded-full">
              <div className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse shadow-[0_0_8px_rgba(16,185,129,0.6)]" />
              <span className="text-xs font-medium text-emerald-400">Auto-Backup Active</span>
            </div>

            <Tooltip>
              <TooltipTrigger render={
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={loadData}
                  disabled={loading}
                  className="rounded-full hover:bg-white/5 text-zinc-400 hover:text-white transition-colors"
                />
              }>
                <RefreshCcw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
              </TooltipTrigger>
              <TooltipContent>Refresh Data</TooltipContent>
            </Tooltip>
          </div>
        </header>

        <main className="relative z-10 flex-1 flex flex-col p-8 gap-8 overflow-hidden">

          {/* Dashboard Summary */}
          {latestSave && (
            <section className="animate-in fade-in slide-in-from-top-4 duration-700">
              <Card className="glass-card overflow-hidden group">
                <div className="md:flex">
                  <div className="relative md:w-72 h-40 md:h-auto shrink-0 overflow-hidden">
                    <SavePreview
                      path={latestSave.path}
                      last_modified={latestSave.last_modified}
                      onExpand={setFullscreenImage}
                      className="w-full h-full border-none rounded-none"
                    />
                    <div className="absolute inset-0 bg-linear-to-t md:bg-linear-to-r from-zinc-900/80 via-transparent to-transparent" />
                  </div>
                  <CardContent className="p-6 flex flex-col justify-center flex-1">
                    <div className="flex flex-wrap items-center gap-3 mb-2">
                      <Badge variant="outline" className="bg-indigo-500/10 text-indigo-400 border-indigo-500/20 px-2 py-0">
                        LATEST SAVE
                      </Badge>
                      <span className="flex items-center text-xs text-zinc-500">
                        <Clock className="w-3 h-3 mr-1" />
                        {formatDate(latestSave.last_modified)}
                      </span>
                    </div>
                    <CardTitle className="text-2xl mb-1 group-hover:text-indigo-300 transition-colors">{latestSave.name}</CardTitle>
                    <CardDescription className="text-zinc-400 text-sm line-clamp-1 flex items-center">
                      <Folder className="w-3 h-3 mr-1" />
                      {latestSave.path}
                    </CardDescription>
                  </CardContent>
                  <CardFooter className="p-6 shrink-0 border-t md:border-t-0 md:border-l border-white/5 flex items-center justify-end">
                    <Button
                      onClick={() => handleBackup(latestSave.name)}
                      disabled={loading}
                      className="bg-indigo-600 hover:bg-indigo-500 text-white shadow-lg shadow-indigo-500/20 px-6 gap-2 h-11"
                    >
                      <Shield className="w-4 h-4" />
                      Quick Backup
                    </Button>
                  </CardFooter>
                </div>
              </Card>
            </section>
          )}

          {/* Repository Lists */}
          <Tabs defaultValue="active" className="flex-1 flex flex-col min-h-0 animate-in fade-in slide-in-from-bottom-4 duration-1000 delay-100">
            <div className="flex items-center justify-between mb-4">
              <TabsList className="bg-zinc-900/60 border border-white/5 p-1">
                <TabsTrigger value="active" className="data-active:bg-indigo-600/20 data-active:text-indigo-300 px-6">
                  <Folder className="w-4 h-4 mr-2" />
                  Active Saves
                  <Badge variant="secondary" className="ml-2 bg-zinc-800 text-zinc-400 border-none px-1.5 h-4 text-[10px]">
                    {saves.length}
                  </Badge>
                </TabsTrigger>
                <TabsTrigger value="backups" className="data-active:bg-purple-600/20 data-active:text-purple-300 px-6">
                  <Archive className="w-4 h-4 mr-2" />
                  Backups
                  <Badge variant="secondary" className="ml-2 bg-zinc-800 text-zinc-400 border-none px-1.5 h-4 text-[10px]">
                    {backups.length}
                  </Badge>
                </TabsTrigger>
              </TabsList>

              <div className="flex items-center text-xs text-zinc-500 bg-zinc-900/20 px-3 py-1.5 rounded-lg border border-white/5">
                <AlertCircle className="w-3 h-3 mr-1.5" />
                Saves are automatically detected from game directory
              </div>
            </div>

            <TabsContent value="active" className="flex-1 flex flex-col min-h-0 mt-0 data-[active=false]:hidden focus-visible:outline-none">
              <Card className="glass-card flex flex-col flex-1 min-h-0 overflow-hidden">
                <ScrollArea className="flex-1 min-h-0">
                  <div className="grid grid-cols-1 lg:grid-cols-2 p-6 gap-4">
                    {saves.length === 0 ? (
                      <div className="col-span-full flex flex-col items-center justify-center py-20 text-zinc-500 bg-black/10 rounded-xl border border-dashed border-white/5">
                        <Folder className="w-12 h-12 mb-4 opacity-20" />
                        <p className="text-sm font-medium">No active saves found</p>
                      </div>
                    ) : (
                      saves.map(save => (
                        <div key={save.name} className="flex items-center p-4 bg-zinc-900/40 border border-white/5 rounded-xl hover:bg-zinc-900/60 transition-all duration-300 group/item">
                          <SavePreview path={save.path} last_modified={save.last_modified} onExpand={setFullscreenImage} />
                          <div className="ml-4 flex-1 min-w-0">
                            <h3 className="font-semibold text-sm truncate group-hover/item:text-indigo-300 transition-colors" title={save.name}>{save.name}</h3>
                            <p className="text-[10px] text-zinc-500 mt-1 flex items-center">
                              <Clock className="w-3 h-3 mr-1" />
                              {formatDate(save.last_modified)}
                            </p>
                          </div>
                          <Tooltip>
                            <TooltipTrigger render={
                              <Button
                                onClick={() => handleBackup(save.name)}
                                disabled={loading}
                                variant="ghost"
                                size="icon"
                                className="ml-2 h-10 w-10 rounded-full hover:bg-indigo-500/20 text-zinc-400 hover:text-indigo-400 border border-transparent hover:border-indigo-500/30 transition-all"
                              />
                            }>
                              <Shield className="w-4 h-4" />
                            </TooltipTrigger>
                            <TooltipContent>Create Backup</TooltipContent>
                          </Tooltip>
                        </div>
                      ))
                    )}
                  </div>
                </ScrollArea>
              </Card>
            </TabsContent>

            <TabsContent value="backups" className="flex-1 flex flex-col min-h-0 mt-0 data-[active=false]:hidden focus-visible:outline-none">
              <Card className="glass-card flex flex-col flex-1 min-h-0 overflow-hidden">
                <ScrollArea className="flex-1 min-h-0">
                  <div className="grid grid-cols-1 lg:grid-cols-2 p-6 gap-4">
                    {backups.length === 0 ? (
                      <div className="col-span-full flex flex-col items-center justify-center py-20 text-zinc-500 bg-black/10 rounded-xl border border-dashed border-white/5">
                        <Archive className="w-12 h-12 mb-4 opacity-20" />
                        <p className="text-sm font-medium">No backups available yet</p>
                      </div>
                    ) : (
                      backups.map(backup => (
                        <div key={backup.name} className="flex items-center p-4 bg-zinc-900/40 border border-white/5 rounded-xl hover:bg-zinc-900/60 transition-all duration-300 group/item">
                          <SavePreview path={backup.path} last_modified={backup.last_modified} onExpand={setFullscreenImage} />
                          <div className="ml-4 flex-1 min-w-0">
                            <h3 className="font-semibold text-sm truncate group-hover/item:text-purple-300 transition-colors" title={backup.name}>{backup.name}</h3>
                            <p className="text-[10px] text-zinc-500 mt-1 flex items-center">
                              <History className="w-3 h-3 mr-1" />
                              {formatDate(backup.last_modified)}
                            </p>
                          </div>
                          <div className="flex gap-1">
                            <Tooltip>
                              <TooltipTrigger render={
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  onClick={() => setConfirmRestore(backup.name)}
                                  disabled={loading}
                                  className="h-10 w-10 rounded-full hover:bg-emerald-500/20 text-zinc-400 hover:text-emerald-400 border border-transparent hover:border-emerald-500/30 transition-all"
                                />
                              }>
                                <ArrowRightCircle className="w-4 h-4" />
                              </TooltipTrigger>
                              <TooltipContent>Restore this Backup</TooltipContent>
                            </Tooltip>

                            <Tooltip>
                              <TooltipTrigger render={
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  onClick={() => setConfirmDelete(backup.name)}
                                  disabled={loading}
                                  className="h-10 w-10 rounded-full hover:bg-red-500/20 text-zinc-400 hover:text-red-400 border border-transparent hover:border-red-500/30 transition-all"
                                />
                              }>
                                <Trash2 className="w-4 h-4" />
                              </TooltipTrigger>
                              <TooltipContent>Delete Backup</TooltipContent>
                            </Tooltip>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                </ScrollArea>
              </Card>
            </TabsContent>
          </Tabs>
        </main>

        <footer className="relative z-20 px-8 py-3 border-t border-white/5 bg-black/20 flex justify-between items-center text-[10px] font-medium text-zinc-600 uppercase tracking-widest">
          <div className="flex items-center gap-4">
            <span>Stable Build v0.1.0</span>
            <div className="w-1 h-1 rounded-full bg-zinc-800" />
            <div className="flex items-center gap-1.5 text-zinc-500">
              <History className="w-3 h-3" />
              Latest Restore: Never
            </div>
          </div>
          <div className="flex items-center gap-1.5 hover:text-indigo-400 transition-colors cursor-pointer group">
            Docs & Help <ExternalLink className="w-2.5 h-2.5 opacity-0 group-hover:opacity-100 transition-opacity" />
          </div>
        </footer>

        {/* Fullscreen Preview */}
        {fullscreenImage && (
          <div
            className="fixed inset-0 z-100 flex items-center justify-center bg-black/95 backdrop-blur-md p-4 sm:p-12 cursor-pointer animate-in fade-in zoom-in-95 duration-300"
            onClick={() => setFullscreenImage(null)}
          >
            <div className="relative max-w-5xl w-full h-full flex flex-col items-center justify-center gap-6">
              <img
                src={fullscreenImage}
                alt="Fullscreen save preview"
                className="max-w-full max-h-[85vh] object-contain rounded-2xl shadow-[0_0_100px_-20px_rgba(99,102,241,0.5)] border border-white/10"
              />
              <div className="text-zinc-400 text-sm font-medium bg-zinc-900/80 px-6 py-3 rounded-full border border-white/10 backdrop-blur-md">
                Click anywhere to close
              </div>
            </div>
          </div>
        )}

        {/* Confirm Dialogs */}
        <AlertDialog open={confirmDelete !== null} onOpenChange={(open) => !open && setConfirmDelete(null)}>
          <AlertDialogContent className="glass border-white/10 text-slate-50">
            <AlertDialogHeader>
              <AlertDialogTitle className="text-xl">Delete Backup?</AlertDialogTitle>
              <AlertDialogDescription className="text-zinc-400">
                Are you sure you want to delete <span className="text-slate-100 font-bold">{confirmDelete}</span>? This action cannot be undone.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel className="bg-transparent border-white/10 hover:bg-white/5 text-slate-100 transition-colors">Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={() => confirmDelete && handleDelete(confirmDelete)}
                className="bg-red-600 hover:bg-red-500 text-white border-none transition-colors"
              >
                Delete Forever
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

        <AlertDialog open={confirmRestore !== null} onOpenChange={(open) => !open && setConfirmRestore(null)}>
          <AlertDialogContent className="glass border-white/10 text-slate-50">
            <AlertDialogHeader>
              <AlertDialogTitle className="text-xl">Restore Backup?</AlertDialogTitle>
              <AlertDialogDescription className="text-zinc-400">
                This will overwrite your <span className="text-indigo-400 font-bold">active save</span> with the selected backup: <span className="text-slate-100 font-bold">{confirmRestore}</span>.
                Existing unsaved progress will be lost.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel className="bg-transparent border-white/10 hover:bg-white/5 text-slate-100 transition-colors">Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={() => confirmRestore && handleRestore(confirmRestore)}
                className="bg-indigo-600 hover:bg-indigo-500 text-white border-none transition-colors"
              >
                Restore Now
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>

      </div>
    </TooltipProvider>
  );
}

export default App;
