import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Folder, Archive, Shield, Trash2, ArrowRightCircle, RefreshCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";

interface SaveFolder {
  name: string;
  path: string;
  last_modified: number;
}

function App() {
  const [saves, setSaves] = useState<SaveFolder[]>([]);
  const [backups, setBackups] = useState<SaveFolder[]>([]);
  const [autoBackup, setAutoBackup] = useState(false);
  const [loading, setLoading] = useState(false);

  const loadData = async () => {
    try {
      const activeSaves: SaveFolder[] = await invoke("get_saves");
      const savedBackups: SaveFolder[] = await invoke("get_backups");
      const autoStatus: boolean = await invoke("get_auto_backup_status");
      
      setSaves(activeSaves);
      setBackups(savedBackups);
      setAutoBackup(autoStatus);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 10000);
    return () => clearInterval(interval);
  }, []);

  const handleBackup = async (name: string) => {
    setLoading(true);
    try {
      await invoke("backup_save", { saveName: name });
      await loadData();
    } catch (e) {
      console.error(e);
      alert(e);
    }
    setLoading(false);
  };

  const handleRestore = async (name: string) => {
    if (!confirm(`Are you sure you want to overwrite your active BG3 save with ${name}?`)) return;
    setLoading(true);
    try {
      await invoke("restore_backup", { backupName: name });
      await loadData();
      alert("Successfully restored backup!");
    } catch (e) {
      console.error(e);
      alert(e);
    }
    setLoading(false);
  };

  const handleDelete = async (name: string) => {
    if (!confirm(`Are you sure you want to delete the backup ${name}?`)) return;
    setLoading(true);
    try {
      await invoke("delete_backup", { backupName: name });
      await loadData();
    } catch (e) {
      console.error(e);
    }
    setLoading(false);
  };

  const toggleAutoBackup = async (checked: boolean) => {
    setAutoBackup(checked);
    try {
      await invoke("toggle_auto_backup", { enabled: checked });
    } catch (err) {
      console.error(err);
      setAutoBackup(!checked);
    }
  };

  const formatDate = (ts: number) => {
    return new Date(ts * 1000).toLocaleString();
  };

  return (
    <div className="dark h-screen overflow-hidden bg-zinc-950 text-slate-50 flex flex-col p-6 font-sans">
      <div className="fixed inset-0 pointer-events-none bg-[radial-gradient(ellipse_at_top,_var(--tw-gradient-stops))] from-zinc-900/40 via-zinc-950 to-zinc-950" />
      
      <header className="relative z-10 flex flex-col sm:flex-row justify-between items-start sm:items-center mb-6 gap-4 sm:gap-0">
        <h1 className="text-3xl sm:text-4xl font-bold bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-transparent">
          BG3 Save Manager
        </h1>
        <div className="flex items-center justify-between sm:justify-start w-full sm:w-auto gap-4 bg-zinc-900/50 border border-white/5 py-3 px-5 rounded-full backdrop-blur-md">
          <span className="text-sm font-medium">Instant Auto Backup</span>
          <Switch 
            checked={autoBackup}
            onCheckedChange={toggleAutoBackup}
            className="data-[state=checked]:bg-emerald-500"
          />
        </div>
      </header>

      <div className="relative z-10 flex flex-col md:grid md:grid-cols-2 md:grid-rows-1 gap-6 flex-1 min-h-0 overflow-y-auto md:overflow-hidden pr-2 -mr-2 pb-2">
        
        <Card className="bg-zinc-900/40 border-white/10 backdrop-blur-xl flex flex-col pt-0 text-slate-50 overflow-hidden min-h-[50vh] md:min-h-0 shrink-0">
          <CardHeader className="flex flex-row items-center border-b border-white/5 pb-4 pt-5 px-6 sticky top-0 bg-zinc-900/80 z-20">
            <Folder className="w-5 h-5 text-indigo-400 mr-3" />
            <div className="flex-1">
              <CardTitle className="text-xl">Active Saves</CardTitle>
            </div>
            <Button variant="ghost" size="icon" onClick={loadData} disabled={loading} className="text-zinc-400 hover:text-white">
              <RefreshCcw className="w-4 h-4" />
            </Button>
          </CardHeader>
          <ScrollArea className="flex-1 min-h-0">
            <div className="p-6 space-y-4">
              {saves.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-48 text-zinc-500 gap-4">
                  <Folder className="w-12 h-12 opacity-50" />
                  <p>No active saves found.</p>
                </div>
              ) : (
                saves.map(save => (
                  <div key={save.name} className="flex justify-between items-center p-4 bg-black/40 border border-white/5 rounded-xl transition-all hover:bg-black/60 hover:-translate-y-0.5 gap-4">
                    <div className="flex-1 min-w-0">
                      <h3 className="font-medium text-[15px] truncate" title={save.name}>{save.name}</h3>
                      <p className="text-xs text-zinc-400 mt-1 truncate">{formatDate(save.last_modified)}</p>
                    </div>
                    <div className="shrink-0">
                      <Button onClick={() => handleBackup(save.name)} disabled={loading} size="sm" className="bg-indigo-600 hover:bg-indigo-500 text-white gap-2">
                        <Shield className="w-4 h-4" />
                        Backup
                      </Button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </ScrollArea>
        </Card>

        <Card className="bg-zinc-900/40 border-white/10 backdrop-blur-xl flex flex-col pt-0 text-slate-50 overflow-hidden min-h-[50vh] md:min-h-0 shrink-0">
          <CardHeader className="flex flex-row items-center border-b border-white/5 pb-4 pt-5 px-6 sticky top-0 bg-zinc-900/80 z-20">
            <Archive className="w-5 h-5 text-purple-400 mr-3" />
            <CardTitle className="text-xl">Backups</CardTitle>
          </CardHeader>
          <ScrollArea className="flex-1 min-h-0">
            <div className="p-6 space-y-4">
              {backups.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-48 text-zinc-500 gap-4">
                  <Archive className="w-12 h-12 opacity-50" />
                  <p>No backups available.</p>
                </div>
              ) : (
                backups.map(backup => (
                  <div key={backup.name} className="flex justify-between items-center p-4 bg-black/40 border border-white/5 rounded-xl transition-all hover:bg-black/60 hover:-translate-y-0.5 gap-4">
                    <div className="flex-1 min-w-0">
                      <h3 className="font-medium text-[15px] truncate" title={backup.name}>{backup.name}</h3>
                      <p className="text-xs text-zinc-400 mt-1 truncate">{formatDate(backup.last_modified)}</p>
                    </div>
                    <div className="flex gap-2 shrink-0">
                      <Button variant="outline" size="sm" onClick={() => handleRestore(backup.name)} disabled={loading} className="bg-transparent border-white/10 hover:bg-white/5 text-slate-100 gap-2">
                        <ArrowRightCircle className="w-4 h-4 text-emerald-400" />
                        Restore
                      </Button>
                      <Button variant="outline" size="sm" onClick={() => handleDelete(backup.name)} disabled={loading} className="bg-transparent border-red-500/30 hover:bg-red-500/10 text-red-500">
                        <Trash2 className="w-4 h-4" />
                      </Button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </ScrollArea>
        </Card>

      </div>
    </div>
  );
}

export default App;
