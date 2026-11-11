# GaLa

A game launcher and downloader for Windows, in one app. GaLa downloads games over BitTorrent, extracts and installs them, keeps your library with proper art and details, launches games and counts your play time, and backs up your saves.

It's built with Tauri 2 (Rust), React and a small C++ download engine on top of libtorrent.

> GaLa downloads whatever torrent you give it. Only download games you have the right to, such as DRM-free games you own, free games and freely shared software.

## What it does

### Download
- Add a game from a `.torrent` file, a magnet link or a drag and drop. Clicking a torrent or magnet link in your browser opens GaLa too.
- **Choose files before downloading.** GaLa fetches the file list first and downloads nothing until you pick. You can tick or untick files later, even after a torrent finishes.
- One download at a time with a queue, speed limits, live speed graph, peers, trackers and a piece map.
- Seeding rules: forever, until a ratio, for a number of hours, or not at all. Stop all seeding from the tray.
- The download engine runs as its own process, so downloads keep going if the window closes or crashes.

### Install
- Finished downloads are recognised as games. Archives (`.zip`, `.7z`, `.rar`, split archives) and disc images (`.iso`) are extracted with 7-Zip, and installers (Inno Setup, NSIS, MSI) run quietly into the folder you choose.
- Password-protected archives ask for the password.
- After installing, GaLa finds the game's real `.exe`, skipping launchers, redistributables, crash reporters and uninstallers.

### Library
- Details and art for every game: summary, developer, release date, genres, rating and screenshots from IGDB, plus covers, banners and logos from SteamGridDB. Steam games are matched by their Steam ID.
- **Find games already on your PC.** Point GaLa at a folder and it lists the games inside to tick and add. It also finds games installed through Steam, Epic Games, Ubisoft Connect and Battle.net, and **Play** opens those in their own launcher.
- Covers, banners and a detailed list view; collections, your own categories, genres, favourites, ratings, notes and progress.
- Right-click any game for Play, Favourite or Remove. Removing can also send the game's folder to the Recycle Bin.
- Move a game to another folder or drive safely: the old copy stays until the new one is complete.

### Play
- Launch options per game, play time tracking, and a **Stop game** button for games that hang.
- A Home page with a rotating hero, your week's play time, download status, free space and shelves like *Jump back in*, *Top rated, not played yet* and *Rediscover*.
- A Stats page with play time by day and your most played games.

### Saves
- Finds where each game keeps its saves using the [Ludusavi manifest](https://github.com/mtkennerly/ludusavi-manifest), and backs them up after every session. Backups are plain folders with timestamps, and a restore backs up your current saves first.

## How it fits together

```
React UI (app/src)
   │  Tauri commands and events
Rust core (app/src-tauri)      library database (SQLite), metadata, installer,
   │                           scanner, saves, moves, seeding rules, tray
   │  JSON lines over the named pipe \\.\pipe\gala-engine
gala-engine (engine/src)       C++ and libtorrent: downloads, seeding, file choices
```

The engine keeps its own state (resume files) and survives the app closing. When the app starts, it connects to a running engine or starts a new one.

## Getting started

### What you need
- Windows 10 or 11, 64-bit
- [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 20 or newer
- Visual Studio 2022 or its Build Tools, with **Desktop development with C++**
- [CMake](https://cmake.org) 3.24 or newer
- [vcpkg](https://github.com/microsoft/vcpkg), for libtorrent and nlohmann-json
- [7-Zip](https://www.7-zip.org), installed in `C:\Program Files\7-Zip`, to extract archives
- Free API keys from [IGDB](https://api-docs.igdb.com/#account-creation) (through Twitch) and [SteamGridDB](https://www.steamgriddb.com/profile/preferences/api), for game details and art

### 1. Set your build folder
Build output and vcpkg live in `E:\GaLa\dev` by default, to keep large builds off the system drive. Change it to suit your PC in four places:

| File | What to change |
|---|---|
| `engine/build.ps1` | `$dev`, the folder for vcpkg and the engine build |
| `app/src-tauri/.cargo/config.toml` | `target-dir`, where Rust builds go |
| `app/package.json` | the engine path in the `bundle` script |
| `app/src-tauri/src/engine.rs` | `DEV_ENGINE`, the engine the app runs in development |

### 2. Add your API keys
Copy `.env.example` to `.env` in the project root and fill in your keys. `.env` is ignored by git, so your keys stay on your PC.

### 3. Build the engine
Clone and bootstrap vcpkg into your build folder, then build:

```powershell
git clone https://github.com/microsoft/vcpkg E:\GaLa\dev\vcpkg
E:\GaLa\dev\vcpkg\bootstrap-vcpkg.bat
.\engine\build.ps1
```

The first build compiles libtorrent and Boost, which takes a while.

### 4. Run the app

```powershell
cd app
npm install
npm run tauri dev
```

In development the app uses the engine from your build folder.

### 5. Make an installer

```powershell
cd app
npm run bundle
```

This builds an NSIS installer that installs GaLa for your Windows user only, with no administrator rights needed. It registers GaLa for `.torrent` files and magnet links, and stops a running engine before updating it. The installer lands in `<target-dir>\release\bundle\nsis\`.

Windows doesn't let an installer change your default app for `.torrent` files, so pick GaLa once in **Settings > Apps > Default apps**.

## Where GaLa keeps things

| What | Where |
|---|---|
| Library database, art, engine state | `%LOCALAPPDATA%\com.gala.launcher\` |
| Settings | `%APPDATA%\com.gala.launcher\settings.json` |
| API keys for an installed copy | `%APPDATA%\com.gala.launcher\.env`, or the project's `.env` |
| Downloads, games and save backups | `E:\GaLa\Downloads`, `E:\GaLa\Games` and `E:\GaLa\Save Backups` by default |

Reinstalling keeps your library and settings.

## Project layout

```
app/
  src/                React UI: pages, components, API types
  src-tauri/src/
    lib.rs            commands, tray, startup, opening torrents and magnet links
    engine.rs         connection to the download engine
    db.rs             SQLite library and play sessions
    library.rs        turning downloads into games, finding a game's .exe
    installer.rs      extracting archives and running installers
    metadata.rs       IGDB and SteamGridDB matching and art
    scanner.rs        finding games on disk and in other launchers
    player.rs         launching games and tracking play time
    saves.rs          save locations and backups
    mover.rs          moving and deleting game folders
    seeding.rs        seeding rules and the tray tooltip
    settings.rs       settings and their defaults
  src-tauri/installer-hooks.nsh   stops the engine during install and uninstall
engine/
  src/main.cpp        the libtorrent engine and its pipe protocol
design/               the original UI mockup
```

## Making it yours

- **Default folders:** change the defaults in `app/src-tauri/src/settings.rs` and the drive shortcuts in `app/src/components/InstallDialog.tsx`.
- **Another launcher:** add it to `installed_launchers` in `scanner.rs`. Each game needs a folder and a link that opens it, such as `steam://rungameid/<id>`.
- **Look and feel:** colours and fonts are CSS variables at the top of `app/src/styles.css`.
- **Engine commands:** the pipe protocol is plain JSON lines, so you can test the engine on its own. Run it with a state folder and a pipe name, such as `gala-engine.exe C:\temp\state \\.\pipe\gala-test`, then send commands like `{"cmd":"snapshot"}`.

## Known limits

- Windows only.
- Games from other launchers don't track play time, back up saves or move, because their launcher runs them.
- EA app and Rockstar Games Launcher games aren't detected yet.
- Some repack installers fail when run quietly. Use **Use the game's own installer** for those.
- If you skip parts of a repack when choosing files, its installer may still try to install them.

## License

No license has been chosen yet, so all rights are reserved for now. Add a `LICENSE` file before sharing GaLa or accepting contributions.
