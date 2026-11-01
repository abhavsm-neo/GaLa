import { convertFileSrc } from '@tauri-apps/api/core';
import { ask, open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { api, type DayPlaytime, type Game, type InstallProgress, type MoveProgress, type Running, type SaveStatus } from '../api';
import { GameArt, installLabel, PlayBox, playable, type GameActions } from '../components/GameCards';
import { InstallDialog } from '../components/InstallDialog';
import { MatchDialog } from '../components/MatchDialog';
import { MoveDialog } from '../components/MoveDialog';
import { bytes, lastPlayed, playtime, tint } from '../format';

const STATUSES = ['Playing', 'Backlog', 'Finished', 'Dropped'];
const SESSION_DAYS = 30;

function localDay(daysAgo: number) {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function Sessions({ game }: { game: Game }) {
  const [days, setDays] = useState<DayPlaytime[]>([]);

  useEffect(() => {
    api.playtimeByDay(game.id, SESSION_DAYS).then(setDays);
  }, [game.id, game.total_seconds]);

  const seconds = Array.from({ length: SESSION_DAYS }, (_, i) =>
    days.find(d => d.day === localDay(SESSION_DAYS - 1 - i))?.seconds ?? 0);
  const top = Math.max(1, ...seconds);

  return (
    <div className="panel">
      <h3>Play sessions, last 30 days</h3>
      <div className="sessions">
        {seconds.map((value, i) => <i key={i} title={playtime(value)} style={{ height: `${(value / top) * 100}%` }} />)}
      </div>
    </div>
  );
}

function Screenshots({ game }: { game: Game }) {
  const [open, setOpen] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  if (!game.screenshots.length) return null;
  return (
    <div className="panel">
      <h3>Screenshots</h3>
      <div className="shots">
        {game.screenshots.map((path, i) => (
          <button key={path} aria-label={`Open screenshot ${i + 1}`} onClick={() => setOpen(path)}>
            <img src={convertFileSrc(path)} alt="" loading="lazy" />
          </button>
        ))}
      </div>
      {open && (
        <div className="overlay open lightbox" role="dialog" aria-label="Screenshot" onClick={() => setOpen(null)}>
          <img src={convertFileSrc(open)} alt="" />
        </div>
      )}
    </div>
  );
}

function Details({ game, onFix }: { game: Game; onFix: () => void }) {
  const released = game.release_date
    ? new Date(game.release_date * 1000).toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric' })
    : '';
  const rows = [
    ['Developer', game.developer],
    ['Publisher', game.publisher],
    ['Released', released],
    ['Genres', game.genres.join(', ')],
    ['Modes', game.modes.join(', ')],
    ['Rating', game.igdb_rating ? `${Math.round(game.igdb_rating)} / 100` : ''],
  ].filter(([, value]) => value);

  return (
    <div className="panel">
      <h3>Details</h3>
      {rows.map(([label, value]) => <div className="kv" key={label}><span>{label}</span><b title={value}>{value}</b></div>)}
      <div style={{ marginTop: 10 }}><button className="btn sm" onClick={onFix}>Wrong game?</button></div>
    </div>
  );
}

function Categories({ game, known }: { game: Game; known: string[] }) {
  const [text, setText] = useState('');

  const add = () => {
    const name = text.trim();
    setText('');
    if (name && !game.categories.includes(name)) api.updateGame(game.id, { categories: [...game.categories, name] });
  };

  return (
    <div className="panel">
      <h3>Your categories</h3>
      {game.categories.length > 0 && (
        <div className="chips" style={{ marginBottom: 9 }}>
          {game.categories.map(name => (
            <button
              key={name}
              className="chip on"
              aria-label={`Remove ${name}`}
              onClick={() => api.updateGame(game.id, { categories: game.categories.filter(c => c !== name) })}
            >
              {name} ×
            </button>
          ))}
        </div>
      )}
      <form onSubmit={e => { e.preventDefault(); add(); }}>
        <input className="field" list="known-categories" aria-label="Add a category" placeholder="Add a category" value={text} onChange={e => setText(e.target.value)} />
        <datalist id="known-categories">{known.map(name => <option key={name} value={name} />)}</datalist>
      </form>
    </div>
  );
}

function SaveBackups({ game, disabled }: { game: Game; disabled: boolean }) {
  const [status, setStatus] = useState<SaveStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const load = () => api.saveStatus(game.id).then(setStatus).catch(e => setError(String(e)));

  useEffect(() => {
    load();
  }, [game.id, game.total_seconds, game.save_folder]);

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    setError('');
    try {
      await work();
      await load();
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };

  const restore = async (created: number, when: string) => {
    const yes = await ask(`Replace ${game.name}'s saves with the backup from ${when}? Your current saves are backed up first.`, { title: 'Restore saves', kind: 'warning' });
    if (yes) run(() => api.restoreSaves(game.id, created));
  };

  const choose = async () => {
    const folder = await open({ directory: true });
    if (folder) await api.setSaveFolder(game.id, folder);
  };

  const when = (created: number) => new Date(created * 1000).toLocaleString('en-GB', { dateStyle: 'medium', timeStyle: 'short' });
  const description = !status
    ? 'Looking for saves'
    : status.found.length
      ? 'GaLa backs these up each time you close the game.'
      : status.listed
        ? "No saves found yet. They'll appear after you've played."
        : "GaLa doesn't know where this game keeps its saves. Choose the folder to back them up.";

  return (
    <div className="panel">
      <h3>Save backups</h3>
      <p>{description}</p>
      {status?.found.slice(0, 3).map(path => <div className="path" key={path} title={path}>{path}</div>)}
      {status?.backups.map(backup => (
        <div className="kv exeopt" key={backup.created}>
          <span>{when(backup.created)}, {backup.label}, {bytes(backup.bytes)}</span>
          <button className="btn sm" disabled={busy || disabled} onClick={() => restore(backup.created, when(backup.created))}>Restore</button>
        </div>
      ))}
      {error && <p className="err">{error}</p>}
      <div className="panelbtns">
        <button className="btn sm" disabled={busy || !status?.found.length} onClick={() => run(() => api.backUpSaves(game.id))}>Back up now</button>
        <button className="btn sm" onClick={choose}>Choose save folder</button>
        {game.save_folder && <button className="btn sm" onClick={() => api.setSaveFolder(game.id, null)}>Find automatically</button>}
        {status && status.backups.length > 0 && <button className="btn sm" onClick={() => api.openFolder(status.folder)}>Open backups</button>}
      </div>
    </div>
  );
}

function ExePicker({ game }: { game: Game }) {
  const [found, setFound] = useState<string[]>([]);

  useEffect(() => {
    api.findExes(game.id).then(setFound);
  }, [game.id]);

  const browse = async () => {
    const exe = await open({ defaultPath: game.install_dir, filters: [{ name: 'Game', extensions: ['exe'] }] });
    if (exe) api.setGameExe(game.id, exe);
  };

  return (
    <div className="panel">
      <h3>Game file</h3>
      <div className="path">{game.exe_path ?? 'Not chosen yet'}</div>
      {found.filter(path => path !== game.exe_path).slice(0, 6).map(path => (
        <div className="kv exeopt" key={path}>
          <span title={path}>{path.slice(game.install_dir.length + 1) || path}</span>
          <button className="btn sm" onClick={() => api.setGameExe(game.id, path)}>Use this</button>
        </div>
      ))}
      <div style={{ marginTop: 10 }}><button className="btn sm" onClick={browse}>Browse for file</button></div>
    </div>
  );
}

function InstallBar({ game, installing, onInstall }: { game: Game; installing: InstallProgress | null; onInstall: () => void }) {
  const here = installing?.game_id === game.id ? installing : null;
  if (['queued', 'extracting', 'installing', 'detecting'].includes(game.install_state)) {
    return (
      <div className="installbar">
        <b>{installLabel(game)?.[0]}{here?.percent != null ? ` ${here.percent}%` : ''}</b>
        <div className="bigbar"><i className={here?.percent == null ? 'busy' : ''} style={{ width: `${here?.percent ?? 100}%` }} /></div>
      </div>
    );
  }
  if (game.install_state === 'failed') {
    return <button className="btn play warn big" onClick={onInstall}>Try again</button>;
  }
  return <button className="btn play big" onClick={onInstall}>Install</button>;
}

function MoveBar({ moving }: { moving: MoveProgress }) {
  const percent = moving.total ? Math.floor((moving.done / moving.total) * 100) : null;
  return (
    <div className="installbar">
      <b>Moving{percent === null ? '' : ` ${percent}%`}</b>
      <div className="bigbar"><i className={percent === null ? 'busy' : ''} style={{ width: `${percent ?? 100}%` }} /></div>
      <button className="btn sm" onClick={() => api.cancelMove()}>Cancel</button>
    </div>
  );
}

export function GamePage({ game, categories, running, installing, moving, installFolder, actions, onRemoved, onInstall }: {
  game: Game | undefined;
  categories: string[];
  running: Running | null;
  installing: InstallProgress | null;
  moving: MoveProgress | null;
  installFolder: string;
  actions: GameActions;
  onRemoved: () => void;
  onInstall: (folder: string) => void;
}) {
  const [tab, setTab] = useState('Overview');
  const [dialog, setDialog] = useState(false);
  const [matching, setMatching] = useState(false);
  const [relocating, setRelocating] = useState(false);
  const [size, setSize] = useState<number | null>(null);

  useEffect(() => {
    setSize(null);
    if (game) api.folderSize(game.install_dir).then(setSize);
  }, [game?.install_dir]);

  if (!game) return <div className="page">That game is no longer in your library.</div>;

  const movingHere = moving?.game_id === game.id ? moving : null;
  const isRunning = running?.game_id === game.id;

  const remove = async () => {
    const yes = await ask(`Remove ${game.name} from your library? Its files stay on disk.`, { title: 'Remove game', kind: 'warning' });
    if (!yes) return;
    await api.removeGame(game.id);
    onRemoved();
  };

  return (
    <div style={{ '--c': tint(game.id) } as React.CSSProperties}>
      <section className="ghero">
        <GameArt game={game} wide />
        {game.logo ? <img className="logo" src={convertFileSrc(game.logo)} alt={game.name} /> : <h1>{game.name}</h1>}
      </section>

      <div className="actionbar">
        {movingHere && <MoveBar moving={movingHere} />}
        {playable(game) && !game.install_state && !movingHere && <PlayBox game={game} running={running} actions={actions} />}
        {game.install_state && <InstallBar game={game} installing={installing} onInstall={() => setDialog(true)} />}
        {!playable(game) && !game.install_state && (
          <button className="btn play warn big" onClick={() => setTab('Launch settings')}>Finish setup</button>
        )}
        <div className="timeplayed">
          <small>Time played</small>
          <span>Last two weeks: {game.recent_seconds ? playtime(game.recent_seconds) : 'None'}</span>
          <span>Total: {game.total_seconds ? playtime(game.total_seconds) : 'None'}</span>
        </div>
        {game.launcher && <div className="stat"><small>Opens in</small><b>{game.launcher}</b></div>}
        <div className="stat"><small>Last played</small><b>{lastPlayed(game.last_played)}</b></div>
        <div className="stat"><small>On disk</small><b>{size === null ? '-' : bytes(size)}</b></div>
        <div className="right">
          <button className={`iconbtn ${game.favorite ? 'on' : ''}`} onClick={() => api.updateGame(game.id, { favorite: !game.favorite })}>
            {game.favorite ? '♥ Favourite' : '♡ Favourite'}
          </button>
          <button className="iconbtn" onClick={() => api.openFolder(game.install_dir)}>Open folder</button>
        </div>
      </div>

      {game.install_state === 'failed' && <div className="installerr">{game.install_error}</div>}
      {dialog && (
        <InstallDialog
          game={game}
          folder={installFolder}
          onClose={() => setDialog(false)}
          onStart={(folder, interactive) => {
            setDialog(false);
            onInstall(folder);
            api.install(game.id, folder, interactive);
          }}
        />
      )}

      {matching && <MatchDialog game={game} onClose={() => setMatching(false)} />}
      {relocating && <MoveDialog game={game} size={size} onClose={() => setRelocating(false)} />}

      <div className="gtabs">
        {['Overview', 'Launch settings'].map(name => (
          <button key={name} className={tab === name ? 'on' : ''} onClick={() => setTab(name)}>{name}</button>
        ))}
      </div>

      {tab === 'Overview' ? (
        <div className="gbody">
          <div>
            {game.meta_state === 'unmatched' && (
              <div className="panel">
                <h3>Game details</h3>
                <p>GaLa couldn't tell which game this is.</p>
                <div style={{ marginTop: 10 }}><button className="btn sm" onClick={() => setMatching(true)}>Find this game</button></div>
              </div>
            )}
            {game.summary && <div className="panel"><h3>About this game</h3><p>{game.summary}</p></div>}
            <Screenshots game={game} />
            <Sessions game={game} />
            <SaveBackups game={game} disabled={isRunning} />
            <div className="panel">
              <h3>Your notes</h3>
              <textarea
                key={game.id}
                className="field"
                style={{ height: 82, padding: 9, resize: 'vertical' }}
                aria-label="Notes"
                placeholder="Add a build, a mod list, or where you left off"
                defaultValue={game.notes}
                onBlur={e => api.updateGame(game.id, { notes: e.target.value })}
              />
            </div>
          </div>
          <aside>
            <div className="panel">
              <h3>Your progress</h3>
              <div className="chips">
                {STATUSES.map(status => (
                  <button key={status} className={`chip ${game.progress === status ? 'on' : ''}`} onClick={() => api.updateGame(game.id, { progress: status })}>
                    {status}
                  </button>
                ))}
              </div>
              <div className="stars" style={{ marginTop: 9 }} aria-label="Your rating">
                {[1, 2, 3, 4, 5].map(n => (
                  <button key={n} className={n <= game.rating ? 'on' : ''} aria-label={`${n} stars`} onClick={() => api.updateGame(game.id, { rating: n === game.rating ? 0 : n })}>★</button>
                ))}
              </div>
            </div>
            <Categories game={game} known={categories} />
            {game.meta_state === 'matched' && <Details game={game} onFix={() => setMatching(true)} />}
          </aside>
        </div>
      ) : (
        <div className="gbody">
          <div>
            {!game.launcher && <ExePicker game={game} />}
            <div className="panel">
              <h3>Location</h3>
              <div className="path">{game.install_dir}</div>
              {game.launcher ? (
                <p style={{ marginTop: 10 }}>{game.launcher} looks after this game, so move or update it there. GaLa only opens it, and playtime isn't counted.</p>
              ) : (
                <div style={{ marginTop: 10 }}>
                  <button
                    className="btn sm"
                    disabled={Boolean(moving) || Boolean(game.install_state) || isRunning || !game.exe_path}
                    onClick={() => setRelocating(true)}
                  >
                    Move game
                  </button>
                </div>
              )}
            </div>
            {!game.launcher && (
              <div className="panel">
                <h3>Launch options</h3>
                <input
                  key={game.id}
                  className="field"
                  aria-label="Launch options"
                  placeholder="-dx12 -skipintro"
                  defaultValue={game.args}
                  onBlur={e => api.updateGame(game.id, { args: e.target.value })}
                />
              </div>
            )}
          </div>
          <aside>
            <div className="panel">
              <h3>This game</h3>
              <input
                key={game.id}
                className="field"
                aria-label="Game name"
                defaultValue={game.name}
                onBlur={e => e.target.value.trim() && api.updateGame(game.id, { name: e.target.value.trim() })}
              />
              <div style={{ marginTop: 10 }}>
                <button className="btn sm danger" onClick={remove}>Remove from library</button>
              </div>
            </div>
          </aside>
        </div>
      )}
    </div>
  );
}
