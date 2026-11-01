import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { api, type Game, type InstallProgress, type MoveProgress, type Running } from './api';

export function useLibrary() {
  const [games, setGames] = useState<Game[]>([]);
  const [running, setRunning] = useState<Running | null>(null);
  const [installing, setInstalling] = useState<InstallProgress | null>(null);
  const [moving, setMoving] = useState<MoveProgress | null>(null);

  useEffect(() => {
    const load = () => api.games().then(setGames);
    const stops = [
      listen('library-changed', load),
      listen<Running | null>('game-status', e => setRunning(e.payload)),
      listen<InstallProgress | null>('install-progress', e => setInstalling(e.payload)),
      listen<MoveProgress | null>('move-progress', e => setMoving(e.payload)),
    ];
    load();
    api.running().then(setRunning);
    api.installStatus().then(setInstalling);
    api.moveStatus().then(setMoving);
    return () => stops.forEach(stop => stop.then(fn => fn()));
  }, []);

  return { games, running, installing, moving };
}
