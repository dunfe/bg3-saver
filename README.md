# BG3 Save Manager

A desktop application for managing **Baldur's Gate 3** save game backups. Built with [Tauri](https://tauri.app/), React, and TypeScript.

## Features

- **Auto-detect active saves** — Scans your BG3 save directory and lists all active save games
- **One-click backup** — Create timestamped backups of any save with a single click
- **Restore backups** — Revert your active save to any previous backup
- **Delete old backups** — Clean up outdated backups to free disk space
- **Auto-refresh** — Save list refreshes every 10 seconds to stay in sync with the game

## Tech Stack

| Layer | Technology |
|---|---|
| **Frontend** | React 18, TypeScript, Tailwind CSS v4 |
| **UI** | shadcn/ui, Lucide icons, Geist font |
| **Desktop** | Tauri 2 (Rust backend) |
| **Build** | Vite 6, Bun |

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (or [Bun](https://bun.sh/))
- [Rust](https://www.rust-lang.org/tools/install) (for Tauri)

### Install dependencies

```bash
bun install
```

### Run in development

```bash
bun run tauri dev
```

### Build for production

```bash
bun run tauri build
```

## Project Structure

```
bg3-save-manager/
├── src/                    # React frontend
│   ├── App.tsx             # Main UI — save list, backup/restore controls
│   ├── components/ui/      # shadcn/ui components
│   └── lib/utils.ts        # Utility functions
├── src-tauri/              # Rust backend (Tauri)
│   ├── src/                # Rust source — filesystem ops, file watching
│   ├── capabilities/       # Tauri capability definitions
│   └── tauri.conf.json     # Tauri app configuration
├── public/                 # Static assets
└── package.json
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
