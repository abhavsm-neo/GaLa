import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useCallback, useEffect, useRef, useState } from 'react';
import { api, type Settings, type Torrent } from './api';
import { AddDialog } from './components/AddDialog';
import { confirmStop, type GameActions } from './components/GameCards';
import { RemoveDialog } from './components/RemoveDialog';
import { isFinished, percent, rate, stateLabel } from './format';
import { Downloads } from './pages/Downloads';
import { GamePage } from './pages/GamePage';
import { Home } from './pages/Home';
import { Library } from './pages/Library';
import { Stats } from './pages/Stats';
import { useEngine } from './useEngine';
import { useLibrary } from './useLibrary';

const PAGES = ['Home', 'Library', 'Downloads', 'Stats'];
type Route = { page: string; game?: number };

function NowPlaying({ name, startedAt, onOpen, onStop }: { name: string; startedAt: number; onOpen: () => void; onStop: () => void }) {
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  const seconds = Math.max(0, Math.floor(now / 1000 - startedAt));
  const clock = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  return (
    <section className="nowplaying">
      <div>
        <b>{name}</b>
        <small><span className="pulse" /> Playing for {clock}</small>
      </div>
      <button className="btn sm" style={{ marginLeft: 'auto' }} onClick={onOpen}>Game page</button>
      <button className="btn sm play stop" onClick={onStop}>Stop game</button>
    </section>
  );
}
const appWindow = getCurrentWindow();

function DownloadPill({ torrents, onClick }: { torrents: Torrent[]; onClick: () => void }) {
  const active = torrents.find(t => !isFinished(t) && !t.paused && !t.queued);
  const waiting = torrents.filter(t => !isFinished(t)).length;
  const down = torrents.reduce((sum, t) => sum + t.down_rate, 0);
  return (
    <button className="dlpill" onClick={onClick}>
      <span className="ring" style={{ '--p': active ? percent(active) : waiting ? 0 : 100 } as React.CSSProperties} />
      <span>
        <b>{active ? active.name.slice(0, 19) : waiting ? `${waiting} paused` : 'All done'}</b>
        <small>{active ? `${stateLabel(active)} · ${rate(down)}` : 'Nothing downloading'}</small>
      </span>
    </button>
  );
}

export function App() {
  const [route, setRoute] = useState<Route>({ page: 'Home' });
  const [back, setBack] = useState<Route[]>([]);
  const [forward, setForward] = useState<Route[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [adding, setAdding] = useState<{ file: string; magnet?: string } | null>(null);
  const [removing, setRemoving] = useState<Torrent | null>(null);
  const [message, setMessage] = useState('');
  const toastTimer = useRef(0);

  const toast = useCallback((text: string) => {
    setMessage(text);
    clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setMessage(''), 3500);
  }, []);

  const { connected, torrents, detail, history } = useEngine(toast);
  const { games, running, installing, moving } = useLibrary();

  const go = (next: Route) => {
    if (next.page === route.page && next.game === route.game) return;
    setBack(old => [...old, route]);
    setForward([]);
    setRoute(next);
  };
  const setPage = (page: string) => go({ page });
  const goBack = () => {
    const previous = back.at(-1);
    if (!previous) return;
    setBack(back.slice(0, -1));
    setForward([...forward, route]);
    setRoute(previous);
  };
  const goForward = () => {
    const next = forward.at(-1);
    if (!next) return;
    setForward(forward.slice(0, -1));
    setBack([...back, route]);
    setRoute(next);
  };

  const actions: GameActions = {
    open: id => go({ page: 'Game', game: id }),
    play: id => {
      api.play(id).catch(e => toast(String(e)));
    },
  };
  const page = route.page;
  const runningGame = games.find(g => g.id === running?.game_id);

  useEffect(() => {
    api.settings().then(setSettings);
  }, []);

  useEffect(() => {
    const stop = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type !== 'drop') return;
      const file = payload.paths.find(path => path.toLowerCase().endsWith('.torrent'));
      if (file) setAdding({ file });
    });
    return () => void stop.then(fn => fn());
  }, []);

  useEffect(() => {
    const collect = async () => {
      const item = await api.takeOpened();
      if (!item) return;
      setAdding(item.toLowerCase().startsWith('magnet:') ? { file: '', magnet: item } : { file: item });
    };
    collect();
    const stop = listen('torrent-opened', collect);
    return () => void stop.then(fn => fn());
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === 'q') api.quit();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const downloading = torrents.filter(t => !isFinished(t)).length;
  const seeding = torrents.filter(t => t.state === 'seeding' && !t.paused).length;

  return (
    <div className="shell">
      <header className="titlebar" data-tauri-drag-region>
        <div className="brand" aria-label="GaLa">
          <svg viewBox="0 0 32 32" aria-hidden="true"><path fill="currentColor" d="M3 3h26v7H10v12h12v-5h-7v-6h14v18H3z" /></svg>
        </div>
        <button className="hist" aria-label="Back" onClick={goBack}>‹</button>
        <button className="hist" aria-label="Forward" onClick={goForward}>›</button>
        {PAGES.map(name => (
          <button key={name} className={`nav ${page === name || (page === 'Game' && name === 'Library') ? 'on' : ''}`} onClick={() => setPage(name)}>{name}</button>
        ))}
        <span className="spacer" data-tauri-drag-region />
        <DownloadPill torrents={torrents} onClick={() => setPage('Downloads')} />
        <button className="iconbtn" onClick={() => setAdding({ file: '' })}>+ Add</button>
        <div className="winctl">
          <button aria-label="Minimize" onClick={() => appWindow.minimize()}>–</button>
          <button aria-label="Maximize" onClick={() => appWindow.toggleMaximize()}>▢</button>
          <button aria-label="Close to tray" className="x" onClick={() => appWindow.close()}>×</button>
        </div>
      </header>

      <main id="view" className={page === 'Library' ? 'flat' : ''}>
        {page === 'Home' && <Home games={games} running={running} actions={actions} goLibrary={() => setPage('Library')} />}
        {page === 'Library' && <Library games={games} actions={actions} />}
        {page === 'Stats' && <Stats games={games} actions={actions} />}
        {page === 'Game' && (
          <GamePage
            key={route.game}
            game={games.find(g => g.id === route.game)}
            categories={[...new Set(games.flatMap(g => g.categories))].sort()}
            running={running}
            installing={installing}
            moving={moving}
            installFolder={settings?.install_dir ?? 'E:\\GaLa\\Games'}
            actions={actions}
            onRemoved={() => setPage('Library')}
            onInstall={folder => settings && setSettings({ ...settings, install_dir: folder })}
          />
        )}
        {page === 'Downloads' && settings && (
          <Downloads
            torrents={torrents}
            detail={detail}
            history={history}
            settings={settings}
            games={games}
            onOpenGame={actions.open}
            onSettings={setSettings}
            onAdd={() => setAdding({ file: '' })}
            onRemove={setRemoving}
          />
        )}
      </main>

      {running && runningGame && (
        <NowPlaying
          key={running.started_at}
          name={runningGame.name}
          startedAt={running.started_at}
          onOpen={() => actions.open(runningGame.id)}
          onStop={() => confirmStop(runningGame.name)}
        />
      )}

      <footer className="footer">
        <span className={connected ? 'ok' : ''}>{connected ? 'Engine connected' : 'Engine starting'}</span>
        <span>{downloading} downloading</span>
        <span>{seeding} seeding</span>
        <span className="end">{settings?.download_dir}</span>
      </footer>

      {adding && settings && (
        <AddDialog
          key={adding.file + adding.magnet}
          settings={settings}
          initialFile={adding.file}
          initialMagnet={adding.magnet ?? ''}
          onClose={() => setAdding(null)}
          onAdded={next => { setSettings(next); setAdding(null); setPage('Downloads'); toast('Added to downloads'); }}
        />
      )}
      {removing && <RemoveDialog torrent={removing} onClose={() => setRemoving(null)} />}
      <div id="toast" role="status" className={message ? 'open' : ''}>{message}</div>
    </div>
  );
}
