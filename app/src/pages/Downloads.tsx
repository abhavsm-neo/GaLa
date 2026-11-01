import { useEffect, useState } from 'react';
import { api, type Detail, type Game, type Settings, type Torrent } from '../api';
import { NetGraph, PieceMap } from '../components/Charts';
import { installLabel } from '../components/GameCards';
import { bytes, eta, isFinished, percent, rate, stateLabel, tint } from '../format';
import type { Sample } from '../useEngine';

const STAGES = ['Download', 'Verify', 'Extract', 'Install', 'Detect', 'Ready'];
const LIMITS = [['Unlimited', 0], ['50 MB/s', 50], ['10 MB/s', 10]] as const;
const SEED_RULES = [
  ['Forever', 'forever'],
  ['Until 1.0 ratio', 'ratio:1'],
  ['Until 2.0 ratio', 'ratio:2'],
  ['For 24 hours', 'hours:24'],
  ['For a week', 'hours:168'],
  ['Stop when done', 'none'],
];
const MB = 1024 * 1024;

type Props = {
  torrents: Torrent[];
  detail: Detail | null;
  history: Sample[];
  settings: Settings;
  games: Game[];
  onOpenGame: (id: number) => void;
  onSettings: (settings: Settings) => void;
  onAdd: () => void;
  onRemove: (torrent: Torrent) => void;
};

function Art({ torrent }: { torrent: Torrent }) {
  return (
    <div className="art" style={{ '--c': tint(torrent.id) } as React.CSSProperties}>
      <span className="fb">{torrent.name}</span>
    </div>
  );
}

function PauseButton({ torrent }: { torrent: Torrent }) {
  const held = torrent.paused || Boolean(torrent.error);
  return (
    <button className="btn sm" onClick={() => (held ? api.resume(torrent.id) : api.pause(torrent.id))}>
      {held ? 'Resume' : 'Pause'}
    </button>
  );
}

function DetailPanel({ torrent, detail }: { torrent: Torrent; detail: Detail | null }) {
  const [tab, setTab] = useState('Peers');

  useEffect(() => {
    api.detail(torrent.id);
    const timer = setInterval(() => api.detail(torrent.id), 2000);
    return () => clearInterval(timer);
  }, [torrent.id]);

  const data = detail?.id === torrent.id ? detail : null;

  return (
    <div className="detail">
      <div className="subt">
        {['Peers', 'Files', 'Trackers', 'Pieces'].map(name => (
          <button key={name} className={tab === name ? 'on' : ''} onClick={() => setTab(name)}>{name}</button>
        ))}
      </div>
      {tab === 'Peers' && [...(data?.peers ?? [])].sort((a, b) => b.down_rate - a.down_rate).slice(0, 12).map(peer => (
        <div className="peer" key={peer.ip}>
          <span>{peer.ip}</span>
          <span>{peer.client || 'Connecting'}</span>
          <span>{rate(peer.down_rate)}</span>
          <span>{Math.round(peer.progress * 100)}% done</span>
        </div>
      ))}
      {tab === 'Files' && data?.files.map(file => (
        <div className="kv" key={file.path}>
          {file.path}<b>{file.size ? Math.floor((file.done / file.size) * 100) : 100}% of {bytes(file.size)}</b>
        </div>
      ))}
      {tab === 'Trackers' && data?.trackers.map(tracker => (
        <div className="kv" key={tracker.url}>{tracker.url}<b>{tracker.verified ? 'Working' : 'Not confirmed'}</b></div>
      ))}
      {tab === 'Trackers' && !data?.trackers.length && <div className="kv">Trackers<b>None, using DHT</b></div>}
      {tab === 'Pieces' && <PieceMap pieces={data?.pieces ?? []} />}
    </div>
  );
}

function ActiveCard({ torrent, detail, onRemove }: { torrent: Torrent; detail: Detail | null; onRemove: Props['onRemove'] }) {
  const [open, setOpen] = useState(false);
  const stage = torrent.state === 'checking' ? 1 : 0;
  return (
    <>
      <section className="active" style={{ '--c': tint(torrent.id) } as React.CSSProperties}>
        <Art torrent={torrent} />
        <div>
          <div className="activehead">
            <h2>{torrent.name}</h2>
            <button className="btn sm" onClick={() => setOpen(!open)}>Details</button>
            <PauseButton torrent={torrent} />
            <button className="btn sm" onClick={() => onRemove(torrent)}>Cancel</button>
          </div>
          <div className="dlsub">
            <span><b>{percent(torrent)}%</b> {stateLabel(torrent)}</span>
            <span><b>{bytes(torrent.done_bytes)}</b> of {bytes(torrent.total_bytes)}</span>
            <span><b>{rate(torrent.down_rate)}</b></span>
            <span><b>{eta(torrent)}</b> left</span>
            <span><b>{torrent.peers}</b> peers, <b>{torrent.seeds}</b> seeds</span>
          </div>
          {torrent.error && <div className="err">{torrent.error}</div>}
          <div className="bigbar"><i style={{ width: `${torrent.progress * 100}%` }} /></div>
          <div className="steps">
            {STAGES.map((name, i) => (
              <span key={name} className={i < stage ? 'done' : i === stage ? 'now' : ''}>{name}</span>
            ))}
          </div>
        </div>
      </section>
      {open && <DetailPanel torrent={torrent} detail={detail} />}
    </>
  );
}

function Row({ torrent, detail, onRemove }: { torrent: Torrent; detail: Detail | null; onRemove: Props['onRemove'] }) {
  const [open, setOpen] = useState(false);
  const finished = isFinished(torrent);
  const status = torrent.state === 'seeding' && !torrent.paused
    ? `Seeding, ${rate(torrent.up_rate)} up`
    : stateLabel(torrent);
  return (
    <>
      <div className={`qitem ${open ? 'open' : ''}`} onClick={() => setOpen(!open)}>
        <span />
        <Art torrent={torrent} />
        <div>
          <b>{torrent.name}</b>
          <small>{bytes(torrent.total_bytes)}</small>
        </div>
        <div className="h"><div className="tinybar"><i style={{ width: `${torrent.progress * 100}%` }} /></div></div>
        <small className="h" title={torrent.error}>{status}</small>
        <div className="rowacts" onClick={e => e.stopPropagation()}>
          {torrent.queued ? (
            <button className="btn sm" onClick={() => api.startNext(torrent.id)}>Start next</button>
          ) : (
            <PauseButton torrent={torrent} />
          )}
          {finished && (
            <button className="btn sm" onClick={() => api.openFolder(torrent.save_path)}>Folder</button>
          )}
          <button className="btn sm" aria-label={`Remove ${torrent.name}`} onClick={() => onRemove(torrent)}>×</button>
        </div>
      </div>
      {open && <DetailPanel torrent={torrent} detail={detail} />}
    </>
  );
}

function Section({ title, hint, items, detail, onRemove }: {
  title: string;
  hint?: string;
  items: Torrent[];
  detail: Detail | null;
  onRemove: Props['onRemove'];
}) {
  if (!items.length) return null;
  return (
    <div className="shelf">
      <div className="shelfhead"><h2>{title}</h2><span>{hint}</span></div>
      <div className="qlist">
        {items.map(t => <Row key={t.id} torrent={t} detail={detail} onRemove={onRemove} />)}
      </div>
    </div>
  );
}

export function Downloads({ torrents, detail, history, settings, games, onOpenGame, onSettings, onAdd, onRemove }: Props) {
  const toInstall = games.filter(g => g.install_state);
  const byQueue = [...torrents].sort((a, b) => a.queue_position - b.queue_position);
  const unfinished = byQueue.filter(t => !isFinished(t));
  const active = unfinished.find(t => !t.paused && !t.queued) ?? unfinished[0];
  const upNext = unfinished.filter(t => t !== active);
  const seeding = byQueue.filter(isFinished);

  const down = torrents.reduce((sum, t) => sum + t.down_rate, 0);
  const up = torrents.reduce((sum, t) => sum + t.up_rate, 0);
  const peers = torrents.reduce((sum, t) => sum + t.peers, 0);
  const received = torrents.reduce((sum, t) => sum + t.all_time_download, 0);
  const sent = torrents.reduce((sum, t) => sum + t.all_time_upload, 0);

  const setLimit = async (megabytes: number) => {
    onSettings(await api.setLimits(megabytes * MB, settings.up_limit));
  };

  return (
    <div className="dlwrap">
      <div className="net">
        <div>
          <div className="nethead">
            <span className="legend"><i style={{ background: 'var(--go)' }} />Download</span><b>{rate(down)}</b>
            <span className="legend"><i style={{ background: 'var(--info)' }} />Upload</span><b>{rate(up)}</b>
            <span style={{ marginLeft: 'auto' }}>Last 2.5 minutes</span>
          </div>
          <NetGraph history={history} />
        </div>
        <div className="netside">
          <div className="kv">Peers<b>{peers}</b></div>
          <div className="kv">Uploaded<b>{bytes(sent)}</b></div>
          <div className="kv">Ratio<b>{received ? (sent / received).toFixed(2) : '-'}</b></div>
          <div className="kv">Speed limit
            <b>
              <select
                className="select"
                style={{ height: 24 }}
                aria-label="Speed limit"
                value={settings.down_limit / MB}
                onChange={e => setLimit(Number(e.target.value))}
              >
                {LIMITS.map(([label, value]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </b>
          </div>
          <div className="kv">Keep seeding
            <b>
              <select
                className="select"
                style={{ height: 24 }}
                aria-label="Keep seeding"
                value={settings.seed_rule}
                onChange={async e => onSettings(await api.setSeedRule(e.target.value))}
              >
                {SEED_RULES.map(([label, value]) => <option key={value} value={value}>{label}</option>)}
              </select>
            </b>
          </div>
        </div>
      </div>

      {toInstall.length > 0 && (
        <div className="shelf">
          <div className="shelfhead"><h2>Needs you</h2><span>Downloaded and ready to install</span></div>
          <div className="qlist">
            {toInstall.map(g => (
              <div className="needs" key={g.id}>
                <b>{g.name}</b>
                <small>{installLabel(g)?.[0]}</small>
                <button className="btn sm play" onClick={() => onOpenGame(g.id)}>Open</button>
              </div>
            ))}
          </div>
        </div>
      )}
      {active && <ActiveCard key={active.id} torrent={active} detail={detail} onRemove={onRemove} />}
      <Section title="Up next" items={upNext} detail={detail} onRemove={onRemove} />
      <Section title="Finished" items={seeding} detail={detail} onRemove={onRemove} />

      {!torrents.length && (
        <div className="empty">
          <h2>Nothing downloading</h2>
          <p>Paste a magnet link or drop a .torrent file anywhere in this window.</p>
          <button className="btn play" onClick={onAdd}>Add a game</button>
        </div>
      )}
    </div>
  );
}
