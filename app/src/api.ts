import { invoke } from '@tauri-apps/api/core';

export type Torrent = {
  id: string;
  name: string;
  state: 'checking' | 'metadata' | 'downloading' | 'writing' | 'finished' | 'seeding' | 'unknown';
  paused: boolean;
  queued: boolean;
  queue_position: number;
  /** Added but waiting for its files to be chosen. */
  holding: boolean;
  error: string;
  save_path: string;
  added_time: number;
  all_time_download: number;
  all_time_upload: number;
  progress: number;
  done_bytes: number;
  total_bytes: number;
  down_rate: number;
  up_rate: number;
  peers: number;
  seeds: number;
};

export type TorrentFile = { path: string; size: number; done: number; wanted: boolean; pad: boolean };

export type Detail = {
  id: string;
  peers: { ip: string; client: string; down_rate: number; up_rate: number; progress: number }[];
  files: TorrentFile[];
  trackers: { url: string; verified: boolean }[];
  pieces: number[];
};

export type Settings = {
  download_dir: string;
  install_dir: string;
  down_limit: number;
  up_limit: number;
  backup_dir: string;
  seed_rule: string;
  news_feeds: NewsFeed[];
};

export type NewsFeed = { name: string; url: string };

export type PcRow = {
  part: string;
  yours: string;
  minimum: string | null;
  recommended: string | null;
  meets_minimum: boolean | null;
  meets_recommended: boolean | null;
};

export type PcCheck = {
  steam_id: string;
  name: string;
  verdict: 'well' | 'lower' | 'below' | 'unknown';
  summary: string;
  rows: PcRow[];
  minimum: string[];
  recommended: string[];
};

export type Article = {
  id: number;
  source: string;
  title: string;
  summary: string;
  url: string;
  image: string | null;
  has_image: boolean;
  published: number;
  game_id: number | null;
  read: boolean;
};

export type Game = {
  id: number;
  torrent_id: string | null;
  name: string;
  install_dir: string;
  exe_path: string | null;
  args: string;
  favorite: boolean;
  progress: string;
  rating: number;
  notes: string;
  added_at: number;
  total_seconds: number;
  recent_seconds: number;
  last_played: number | null;
  source_path: string | null;
  install_state: '' | 'pending' | 'queued' | 'extracting' | 'installing' | 'detecting' | 'failed';
  install_error: string;
  meta_state: '' | 'matched' | 'unmatched';
  igdb_id: number | null;
  summary: string;
  developer: string;
  publisher: string;
  release_date: number | null;
  igdb_rating: number | null;
  genres: string[];
  modes: string[];
  categories: string[];
  cover: string | null;
  hero: string | null;
  logo: string | null;
  screenshots: string[];
  save_folder: string | null;
  launcher: string;
  launch_uri: string | null;
};

export type Found = {
  name: string;
  folder: string;
  exe: string | null;
  launcher: string;
  launch_uri: string | null;
  installable: boolean;
  sure: boolean;
  added: boolean;
};

export type MoveProgress = { game_id: number; done: number; total: number };
export type Backup = { created: number; label: string; bytes: number };
export type SaveStatus = { listed: boolean; found: string[]; backups: Backup[]; folder: string };

export type Match = { igdb_id: number; name: string; year: number | null; developer: string; cover: string | null };

export type InstallProgress = { game_id: number; stage: string; percent: number | null };

export type GamePatch = Partial<Pick<Game, 'name' | 'args' | 'favorite' | 'progress' | 'rating' | 'notes' | 'categories'>>;
export type Running = { game_id: number; started_at: number };
export type DayPlaytime = { day: string; seconds: number };

export const api = {
  snapshot: () => invoke<boolean>('engine_snapshot'),
  addMagnet: (magnet: string, savePath: string, request: string) => invoke<void>('add_magnet', { magnet, savePath, request }),
  addTorrentFile: (path: string, savePath: string, request: string) => invoke<void>('add_torrent_file', { path, savePath, request }),
  setFiles: (id: string, wanted: boolean[]) => invoke<void>('set_files', { id, wanted }),
  pause: (id: string) => invoke<void>('pause', { id }),
  resume: (id: string) => invoke<void>('resume', { id }),
  startNext: (id: string) => invoke<void>('start_next', { id }),
  remove: (id: string, deleteFiles: boolean) => invoke<void>('remove', { id, deleteFiles }),
  detail: (id: string) => invoke<void>('detail', { id }),
  settings: () => invoke<Settings>('get_settings'),
  setLimits: (down: number, up: number) => invoke<Settings>('set_limits', { down, up }),
  setDownloadDir: (path: string) => invoke<Settings>('set_download_dir', { path }),
  quit: () => invoke<void>('quit'),
  openFolder: (path: string) => invoke<void>('open_folder', { path }),
  games: () => invoke<Game[]>('list_games'),
  addGame: (file: string) => invoke<number>('add_game', { file }),
  install: (id: number, folder: string, interactive: boolean, password = '') =>
    invoke<void>('install_game', { id, folder, interactive, password: password || null }),
  installStatus: () => invoke<InstallProgress | null>('install_status'),
  freeSpace: (path: string) => invoke<number>('free_space', { path }),
  updateGame: (id: number, patch: GamePatch) => invoke<void>('update_game', {
    id,
    patch: { name: null, args: null, favorite: null, progress: null, rating: null, notes: null, categories: null, ...patch },
  }),
  takeOpened: () => invoke<string | null>('take_opened'),
  scanGames: (folders: string[], launchers: boolean) => invoke<Found[]>('scan_games', { folders, launchers }),
  addScanned: (games: Found[]) => invoke<void>('add_scanned', { games }),
  moveGame: (id: number, folder: string) => invoke<void>('move_game', { id, folder }),
  cancelMove: () => invoke<void>('cancel_move'),
  moveStatus: () => invoke<MoveProgress | null>('move_status'),
  saveStatus: (id: number) => invoke<SaveStatus>('save_status', { id }),
  backUpSaves: (id: number) => invoke<void>('back_up_saves', { id }),
  restoreSaves: (id: number, created: number) => invoke<void>('restore_saves', { id, created }),
  setSaveFolder: (id: number, folder: string | null) => invoke<void>('set_save_folder', { id, folder }),
  setSeedRule: (rule: string) => invoke<Settings>('set_seed_rule', { rule }),
  searchMetadata: (query: string) => invoke<Match[]>('search_metadata', { query }),
  chooseMetadata: (id: number, igdbId: number) => invoke<void>('choose_metadata', { id, igdbId }),
  setGameExe: (id: number, exe: string) => invoke<void>('set_game_exe', { id, exe }),
  removeGame: (id: number, deleteFiles: boolean) => invoke<void>('remove_game', { id, deleteFiles }),
  findExes: (id: number) => invoke<string[]>('find_exes', { id }),
  folderSize: (path: string) => invoke<number>('folder_size', { path }),
  play: (id: number) => invoke<void>('play_game', { id }),
  stop: () => invoke<void>('stop_game'),
  running: () => invoke<Running | null>('running_game'),
  pcCheck: (id: number) => invoke<PcCheck | null>('pc_check', { id }),
  pcCheckName: (name: string) => invoke<PcCheck | null>('pc_check_name', { name }),
  pcVerdicts: () => invoke<[number, string, string][]>('pc_verdicts'),
  newsList: () => invoke<Article[]>('news_list'),
  newsImage: (id: number) => invoke<string | null>('news_image', { id }),
  newsMarkRead: (ids: number[]) => invoke<void>('news_mark_read', { ids }),
  newsMarkAllRead: () => invoke<void>('news_mark_all_read'),
  newsRefresh: () => invoke<void>('news_refresh'),
  checkNewsFeed: (url: string) => invoke<string>('check_news_feed', { url }),
  setNewsFeeds: (feeds: NewsFeed[]) => invoke<Settings>('set_news_feeds', { feeds }),
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  playtimeByDay: (gameId: number | null, days: number) => invoke<DayPlaytime[]>('playtime_by_day', { gameId, days }),
};
