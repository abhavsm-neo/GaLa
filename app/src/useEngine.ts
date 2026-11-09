import { listen } from '@tauri-apps/api/event';
import { useEffect, useRef, useState } from 'react';
import { api, type Detail, type Torrent } from './api';

export type Sample = { down: number; up: number };

const HISTORY = 150;

type EngineEvent = Partial<Torrent> & Partial<Detail> & { event: string; id?: string; message?: string; request?: string };

export function useEngine(toast: (message: string) => void) {
  const [connected, setConnected] = useState(false);
  const [torrents, setTorrents] = useState<Record<string, Torrent>>({});
  const [detail, setDetail] = useState<Detail | null>(null);
  const [history, setHistory] = useState<Sample[]>([]);
  const latest = useRef(torrents);
  latest.current = torrents;
  const toastRef = useRef(toast);
  toastRef.current = toast;

  useEffect(() => {
    const stops = [
      listen<boolean>('engine-status', e => setConnected(e.payload)),
      listen<EngineEvent>('engine-event', ({ payload: m }) => {
        const id = m.id ?? '';
        if (m.event === 'ready') setTorrents({});
        if (m.event === 'progress') setTorrents(all => ({ ...all, [id]: m as unknown as Torrent }));
        if (m.event === 'detail') setDetail(m as unknown as Detail);
        if (m.event === 'finished') toastRef.current(`${latest.current[id]?.name ?? 'Download'} finished`);
        // An answer to a request goes to whoever asked, such as the Add dialog.
        if (m.event === 'error' && m.message && !m.request) toastRef.current(m.message);
        if (m.event === 'removed') {
          setTorrents(all => {
            const { [id]: _, ...rest } = all;
            return rest;
          });
        }
      }),
    ];
    // Listeners are in place, so nothing in the snapshot can be missed.
    Promise.all(stops).then(() => api.snapshot().then(setConnected));
    return () => stops.forEach(stop => stop.then(fn => fn()));
  }, []);

  useEffect(() => {
    const timer = setInterval(() => {
      const list = Object.values(latest.current);
      const sample = {
        down: list.reduce((sum, t) => sum + t.down_rate, 0),
        up: list.reduce((sum, t) => sum + t.up_rate, 0),
      };
      setHistory(old => [...old.slice(1 - HISTORY), sample]);
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  return { connected, torrents: Object.values(torrents), detail, history };
}
