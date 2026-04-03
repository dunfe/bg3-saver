import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Folder, Archive, Shield, Trash2, ArrowRightCircle, RefreshCcw } from "lucide-react";
import "./App.css";

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

  const toggleAutoBackup = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const enabled = e.target.checked;
    setAutoBackup(enabled);
    try {
      await invoke("toggle_auto_backup", { enabled });
    } catch (err) {
      console.error(err);
      setAutoBackup(!enabled);
    }
  };

  const formatDate = (ts: number) => {
    return new Date(ts * 1000).toLocaleString();
  };

  return (
    <div className="app-container">
      <header>
        <h1 className="title">BG3 Save Manager</h1>
        <label className={`auto-backup-toggle ${autoBackup ? 'active' : ''}`}>
          <span>1-Min Auto Backup</span>
          <div className="switch">
            <input type="checkbox" checked={autoBackup} onChange={toggleAutoBackup} />
            <span className="slider"></span>
          </div>
        </label>
      </header>

      <div className="main-content">
        <div className="panel">
          <div className="panel-header">
            <Folder className="icon" />
            <h2>Active Saves</h2>
            <button onClick={loadData} disabled={loading} style={{ marginLeft: "auto", padding: "6px" }}>
              <RefreshCcw size={16} />
            </button>
          </div>
          <div className="list-container">
            {saves.length === 0 ? (
              <div className="empty-state">
                <Folder size={48} opacity={0.5} />
                <p>No active saves found.</p>
              </div>
            ) : (
              saves.map(save => (
                <div key={save.name} className="save-card">
                  <div className="save-info">
                    <h3>{save.name}</h3>
                    <p>{formatDate(save.last_modified)}</p>
                  </div>
                  <div className="actions">
                    <button className="primary" onClick={() => handleBackup(save.name)} disabled={loading}>
                      <Shield size={16} />
                      Backup
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        <div className="panel">
          <div className="panel-header">
            <Archive className="icon" />
            <h2>Backups</h2>
          </div>
          <div className="list-container">
            {backups.length === 0 ? (
              <div className="empty-state">
                <Archive size={48} opacity={0.5} />
                <p>No backups available.</p>
              </div>
            ) : (
              backups.map(backup => (
                <div key={backup.name} className="save-card">
                  <div className="save-info">
                    <h3>{backup.name}</h3>
                    <p>{formatDate(backup.last_modified)}</p>
                  </div>
                  <div className="actions">
                    <button onClick={() => handleRestore(backup.name)} disabled={loading}>
                      <ArrowRightCircle size={16} />
                      Restore
                    </button>
                    <button className="danger" onClick={() => handleDelete(backup.name)} disabled={loading}>
                      <Trash2 size={16} />
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
