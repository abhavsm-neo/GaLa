import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { api, type Game, type InstallProgress, type Running } from './api';

export function useLibrary() {
  const [games, setGames] = useState<Game[]>([]);
  const [running, setRunning] = useState<Running | null>(null);
  const [installing, setInstalling] = useState<InstallProgress | null>(null);

  useEffect(() => {
    const load = () => api.games().then(setGames);
    const stops = [
      listen('library-changed', load),
      listen<Running | null>('game-status', e => setRunning(e.payload)),
      listen<InstallProgress | null>('install-progress', e => setInstalling(e.payload)),
    ];
    load();
    api.running().then(setRunning);
    api.installStatus().then(setInstalling);
    return () => stops.forEach(stop => stop.then(fn => fn()));
  }, []);

  return { games, running, installing };
}
