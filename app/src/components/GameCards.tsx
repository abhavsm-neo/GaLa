import { convertFileSrc } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import type { Game, Running } from '../api';
import { api } from '../api';
import { lastPlayed, playtime, tint } from '../format';

export type GameActions = {
  open: (id: number) => void;
  play: (id: number) => void;
  menu: (id: number, e: React.MouseEvent) => void;
  remove: (id: number) => void;
};

export async function confirmStop(name: string) {
  const yes = await ask(`Force ${name} to close? Anything unsaved in the game will be lost.`, { title: 'Stop game', kind: 'warning' });
  if (yes) api.stop();
}

const BUSY: Record<string, string> = {
  queued: 'Queued',
  extracting: 'Extracting',
  installing: 'Installing',
  detecting: 'Finding game',
};

export const playable = (game: Game) => Boolean(game.exe_path || game.launch_uri);

export function installLabel(game: Game): [string, string] | null {
  if (BUSY[game.install_state]) return [BUSY[game.install_state], 'warn'];
  if (game.install_state === 'pending') return ['Ready to install', 'info'];
  if (game.install_state === 'failed') return ['Install failed', 'bad'];
  return playable(game) ? null : ['Needs setup', 'bad'];
}

export function GameArt({ game, wide }: { game: Game; wide?: boolean }) {
  const image = wide ? game.hero ?? game.cover : game.cover ?? game.hero;
  return (
    <span className="art" style={{ '--c': tint(game.id) } as React.CSSProperties}>
      {image ? <img src={convertFileSrc(image)} alt="" loading="lazy" /> : <span className="fb">{game.name}</span>}
    </span>
  );
}

const PREVIEW_MS = 1300;

export function Capsule({ game, actions, rated }: { game: Game; actions: GameActions; rated?: boolean }) {
  const label = installLabel(game);
  return (
    <article
      className="capsule"
      tabIndex={0}
      style={{ '--c': tint(game.id) } as React.CSSProperties}
      onClick={() => actions.open(game.id)}
      onContextMenu={e => actions.menu(game.id, e)}
      onKeyDown={e => e.key === 'Enter' && actions.open(game.id)}
    >
      {label && <span className={`badge ${label[1]}`}>{label[0]}</span>}
      {game.favorite && <span className="fav" aria-label="Favourite">♥</span>}
      <div style={{ position: 'relative' }}>
        <GameArt game={game} />
        {rated && game.igdb_rating != null && <span className="score">{Math.round(game.igdb_rating)}</span>}
        <div className="hov" style={{ borderRadius: 9 }}>
          <small>{playtime(game.total_seconds)}</small>
          {playable(game) ? (
            <button className="btn play" onClick={e => { e.stopPropagation(); actions.play(game.id); }}>Play</button>
          ) : (
            <span className="btn play warn">{game.install_state === 'pending' ? 'Install' : 'Open'}</span>
          )}
        </div>
      </div>
      <div className="ctitle">{game.name}</div>
      <div className="csub">
        {game.total_seconds ? `${playtime(game.total_seconds)} · ${lastPlayed(game.last_played)}` : game.launcher || game.progress}
      </div>
    </article>
  );
}

/** Cycles through the game's screenshots while the pointer rests on it. */
function usePreview(shots: string[]) {
  const [hovered, setHovered] = useState(false);
  const [shot, setShot] = useState(-1);

  useEffect(() => {
    if (!hovered || !shots.length) return setShot(-1);
    const timer = setInterval(() => setShot(i => (i + 1) % shots.length), PREVIEW_MS);
    return () => clearInterval(timer);
  }, [hovered, shots.length]);

  return { shot: shot >= 0 ? shots[shot] : null, onMouseEnter: () => setHovered(true), onMouseLeave: () => setHovered(false) };
}

export function Banner({ game, actions }: { game: Game; actions: GameActions }) {
  const { shot, onMouseEnter, onMouseLeave } = usePreview(game.screenshots);
  return (
    <article
      className="cont"
      tabIndex={0}
      style={{ '--c': tint(game.id) } as React.CSSProperties}
      onClick={() => actions.open(game.id)}
      onContextMenu={e => actions.menu(game.id, e)}
      onKeyDown={e => e.key === 'Enter' && actions.open(game.id)}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      <div className="preview">
        <GameArt game={game} wide />
        {shot && <img key={shot} className="shot" src={convertFileSrc(shot)} alt="" />}
      </div>
      {playable(game) && (
        <button className="quick" aria-label={`Play ${game.name}`} onClick={e => { e.stopPropagation(); actions.play(game.id); }}>▶</button>
      )}
      <div className="info">
        <b>{game.name}</b>
        <small>{playtime(game.total_seconds)} · {lastPlayed(game.last_played)}</small>
      </div>
    </article>
  );
}

export function PlayBox({ game, running, actions }: { game: Game; running: Running | null; actions: GameActions }) {
  if (running?.game_id === game.id) {
    return <button className="btn play stop big" onClick={() => confirmStop(game.name)}>Stop</button>;
  }
  return (
    <div className="playbox">
      <button className="pb-play" aria-label={`Play ${game.name}`} onClick={() => actions.play(game.id)}>
        <svg viewBox="0 0 27 31" aria-hidden="true"><path fill="currentColor" d="M1 1l25 14.5L1 30z" /></svg>
      </button>
    </div>
  );
}
