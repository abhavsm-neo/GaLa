import { convertFileSrc } from '@tauri-apps/api/core';
import { useEffect, useMemo, useState } from 'react';
import { api, type Article, type DayPlaytime, type Game, type Running, type Torrent } from '../api';
import { Banner, Capsule, GameArt, PlayBox, installLabel, playable, type GameActions } from '../components/GameCards';
import { NewsRow } from '../components/NewsCards';
import { gamesIn } from '../useNews';
import { bytes, eta, isFinished, lastPlayed, percent, playtime, stateLabel } from '../format';

type Props = {
  games: Game[];
  running: Running | null;
  torrents: Torrent[];
  downloadDir: string;
  actions: GameActions;
  goLibrary: () => void;
  goDownloads: () => void;
  articles: Article[];
  markRead: (ids: number[]) => void;
  goNews: () => void;
};

const ROTATE_MS = 9000;
const MONTH = 30 * 86400;
const LOW_SPACE = 50 * 1024 ** 3;
const FEW_HOURS = 5 * 3600;
const WIDE = { '--w': '310px' } as React.CSSProperties;

type Slide = { game: Game; kicker: string };

const art = (game: Game) => game.hero ?? game.cover;
const year = (game: Game) => (game.release_date ? new Date(game.release_date * 1000).getFullYear() : null);
const dayKey = (date: Date) => date.toLocaleDateString('en-CA');

function greeting() {
  const hour = new Date().getHours();
  if (hour < 5) return 'Still up?';
  if (hour < 12) return 'Good morning';
  if (hour < 18) return 'Good afternoon';
  return 'Good evening';
}

/** Days in a row with some play, counting back from today, or from yesterday if today has none yet. */
function streak(days: DayPlaytime[]) {
  const played = new Set(days.filter(d => d.seconds > 0).map(d => d.day));
  const date = new Date();
  if (!played.has(dayKey(date))) date.setDate(date.getDate() - 1);
  let count = 0;
  while (played.has(dayKey(date))) {
    count++;
    date.setDate(date.getDate() - 1);
  }
  return count;
}

function Hero({ slides, index, onPick, running, actions }: {
  slides: Slide[];
  index: number;
  onPick: (index: number) => void;
  running: Running | null;
  actions: GameActions;
}) {
  const { game, kicker } = slides[index];
  const facts = [game.genres[0], year(game), game.igdb_rating ? `${Math.round(game.igdb_rating)} / 100` : null].filter(Boolean);
  return (
    <section className="ghero homehero">
      <div className="drift" key={game.id}><GameArt game={game} wide /></div>
      <div className="herotext" key={`text-${game.id}`}>
        <div className="kicker">{kicker}</div>
        {game.logo ? <img className="herologo" src={convertFileSrc(game.logo)} alt={game.name} /> : <h1>{game.name}</h1>}
        {facts.length > 0 && <div className="facts">{facts.map(fact => <span key={String(fact)}>{fact}</span>)}</div>}
        {game.summary && <p className="summary">{game.summary}</p>}
        <div className="meta">
          <span><b>{playtime(game.total_seconds)}</b></span>
          <span>Last played {lastPlayed(game.last_played).toLowerCase()}</span>
        </div>
        <div className="herobtns">
          {playable(game) && <PlayBox game={game} running={running} actions={actions} />}
          <button className="btn" style={{ height: 52 }} onClick={() => actions.open(game.id)}>Game details</button>
        </div>
      </div>
      {slides.length > 1 && (
        <div className="dots">
          {slides.map((slide, i) => (
            <button key={slide.game.id} className={i === index ? 'on' : ''} aria-label={`Show ${slide.game.name}`} onClick={() => onPick(i)} />
          ))}
        </div>
      )}
    </section>
  );
}

function Glance({ games, torrents, downloadDir, days, goDownloads }: {
  games: Game[];
  torrents: Torrent[];
  downloadDir: string;
  days: DayPlaytime[];
  goDownloads: () => void;
}) {
  const [free, setFree] = useState<number | null>(null);

  useEffect(() => {
    api.freeSpace(downloadDir).then(setFree);
  }, [downloadDir]);

  const week = Array.from({ length: 7 }, (_, i) => {
    const date = new Date();
    date.setDate(date.getDate() - 6 + i);
    const seconds = days.find(d => d.day === dayKey(date))?.seconds ?? 0;
    return { label: date.toLocaleDateString(undefined, { weekday: 'narrow' }), seconds };
  });
  const most = Math.max(...week.map(d => d.seconds), 1);
  const total = week.reduce((sum, d) => sum + d.seconds, 0);

  const active = torrents.find(t => !isFinished(t) && !t.paused && !t.queued) ?? torrents.find(t => !isFinished(t));
  const ready = games.filter(g => g.install_state === 'pending' || g.install_state === 'failed').length;

  return (
    <section className="glance">
      <div className="gcard">
        <small>Your week</small>
        <b>{total ? playtime(total) : 'Nothing yet'}</b>
        <div className="weekbars">
          {week.map((d, i) => (
            <span key={i} title={playtime(d.seconds)}>
              <i style={{ height: `${Math.max(4, (d.seconds / most) * 100)}%` }} className={d.seconds ? '' : 'none'} />
              <em>{d.label}</em>
            </span>
          ))}
        </div>
      </div>
      <button className="gcard" onClick={goDownloads}>
        <small>Downloads</small>
        {active ? (
          <>
            <b title={active.name}>{active.name}</b>
            <div className="tinybar"><i style={{ width: `${percent(active)}%` }} /></div>
            <span className="sub">{percent(active)}% · {stateLabel(active)}{active.down_rate ? ` · ${eta(active)} left` : ''}</span>
          </>
        ) : (
          <>
            <b>{ready ? `${ready} ready to install` : 'All caught up'}</b>
            <span className="sub">{ready ? 'Open Downloads to finish them' : 'Nothing downloading right now'}</span>
          </>
        )}
      </button>
      <div className={`gcard ${free !== null && free < LOW_SPACE ? 'low' : ''}`}>
        <small>Space on {downloadDir.slice(0, 2)}</small>
        <b>{free === null ? '-' : `${bytes(free)} free`}</b>
        <span className="sub">{free !== null && free < LOW_SPACE ? 'Running low. Move or remove a game' : 'Where new downloads go'}</span>
      </div>
    </section>
  );
}

function Shelf({ title, note, wide, games, actions, index }: {
  title: string;
  note?: string;
  wide?: boolean;
  games: Game[];
  actions: GameActions;
  index: number;
}) {
  if (!games.length) return null;
  return (
    <section className="shelf rise" style={{ '--i': index } as React.CSSProperties}>
      <div className="shelfhead"><h2>{title}</h2>{note && <span>{note}</span>}</div>
      <div className="row" style={wide ? WIDE : undefined}>
        {games.map(g => (wide ? <Banner key={g.id} game={g} actions={actions} /> : <Capsule key={g.id} game={g} actions={actions} rated />))}
      </div>
    </section>
  );
}

export function Home({ games, running, torrents, downloadDir, actions, goLibrary, goDownloads, articles, markRead, goNews }: Props) {
  const [index, setIndex] = useState(0);
  const [pick, setPick] = useState<Game | null>(null);
  const [paused, setPaused] = useState(false);
  const [days, setDays] = useState<DayPlaytime[]>([]);

  useEffect(() => {
    api.playtimeByDay(null, 60).then(setDays);
  }, [running]);

  const now = Date.now() / 1000;
  const played = useMemo(() => games.filter(g => g.last_played).sort((a, b) => b.last_played! - a.last_played!), [games]);
  const added = useMemo(() => [...games].sort((a, b) => b.added_at - a.added_at), [games]);
  const backlog = useMemo(
    () => games.filter(g => playable(g) && !g.total_seconds).sort((a, b) => (b.igdb_rating ?? 0) - (a.igdb_rating ?? 0)),
    [games],
  );
  const favourites = games.filter(g => g.favorite);
  const rediscover = games
    .filter(g => g.last_played && now - g.last_played > MONTH)
    .sort((a, b) => b.total_seconds - a.total_seconds);

  const genre = useMemo(() => {
    const hours = new Map<string, number>();
    for (const g of games) for (const name of g.genres) hours.set(name, (hours.get(name) ?? 0) + g.total_seconds);
    const [top] = [...hours].filter(([, seconds]) => seconds > 0).sort((a, b) => b[1] - a[1]);
    return top?.[0];
  }, [games]);
  const sameGenre = genre
    ? games.filter(g => g.genres.includes(genre) && g.total_seconds < FEW_HOURS).sort((a, b) => a.total_seconds - b.total_seconds || (b.igdb_rating ?? 0) - (a.igdb_rating ?? 0))
    : [];

  const slides = useMemo(() => {
    const list: Slide[] = [];
    const push = (game: Game | undefined, kicker: string) => {
      if (game && art(game) && !list.some(s => s.game.id === game.id)) list.push({ game, kicker });
    };
    push(played[0], 'Continue playing');
    push(added[0], 'New in your library');
    push(games.filter(g => g.favorite).sort((a, b) => (a.last_played ?? 0) - (b.last_played ?? 0))[0], 'A favourite of yours');
    push(backlog[0], 'Top rated, not played yet');
    if (!list.length) list.push({ game: played[0] ?? added[0], kicker: played[0] ? 'Continue playing' : 'New in your library' });
    return list;
  }, [games, played, added, backlog]);

  useEffect(() => {
    if (paused || pick || slides.length < 2) return;
    const timer = setTimeout(() => setIndex(i => (i + 1) % slides.length), ROTATE_MS);
    return () => clearTimeout(timer);
  }, [index, paused, pick, slides.length]);

  if (!games.length) {
    return (
      <div className="empty">
        <h2>Your library is empty</h2>
        <p>Games appear here once a download finishes, or add one you already have.</p>
        <button className="btn play" onClick={goLibrary}>Go to library</button>
      </div>
    );
  }

  const shown: Slide[] = pick ? [{ game: pick, kicker: "Tonight's pick" }] : slides;
  const current = shown[Math.min(index, shown.length - 1)];
  const choices = backlog.length ? backlog : games.filter(playable);
  const roll = () => {
    const others = choices.filter(g => g.id !== pick?.id);
    const pool = others.length ? others : choices;
    setPick(pool[Math.floor(Math.random() * pool.length)]);
    setIndex(0);
  };
  const days7 = streak(days);
  const needs = games.filter(g => installLabel(g)?.[1] === 'bad').length;

  return (
    <div className="home">
      {art(current.game) && (
        <div className="homebg" key={current.game.id}>
          <img src={convertFileSrc(art(current.game)!)} alt="" />
        </div>
      )}
      <div onMouseEnter={() => setPaused(true)} onMouseLeave={() => setPaused(false)}>
        <Hero slides={shown} index={Math.min(index, shown.length - 1)} onPick={i => { setPick(null); setIndex(i); }} running={running} actions={actions} />
      </div>
      <div className="page" style={{ paddingTop: 0, position: 'relative' }}>
        <div className="hello rise">
          <h2>{greeting()}</h2>
          <span>
            {days7 > 1 ? `${days7} days in a row` : played.length ? `${games.length} games in your library` : 'Pick something to start'}
            {needs > 0 ? ` · ${needs} need attention` : ''}
          </span>
          {choices.length > 0 && (
            <button className="btn sm" onClick={roll}>{pick ? 'Pick another' : "Can't decide?"}</button>
          )}
          {pick && <button className="btn sm" onClick={() => setPick(null)}>Back</button>}
        </div>
        <Glance games={games} torrents={torrents} downloadDir={downloadDir} days={days} goDownloads={goDownloads} />
        {articles.length > 0 && (
          <section className="shelf rise homenews" style={{ '--i': 0 } as React.CSSProperties}>
            <div className="shelfhead"><h2>Latest news</h2><button onClick={goNews}>See all</button></div>
            <div className="nrows">
              {articles.slice(0, 6).map(a => <NewsRow key={a.id} article={a} games={gamesIn(a, games)} markRead={markRead} />)}
            </div>
          </section>
        )}
        <Shelf index={0} title="Jump back in" note="Your recent sessions" wide games={played.slice(0, 6)} actions={actions} />
        <Shelf index={1} title="Favourites" games={favourites} actions={actions} />
        <Shelf index={2} title="New in your library" games={added.slice(0, 10)} actions={actions} />
        <Shelf index={3} title="Top rated, not played yet" note={`${backlog.length} waiting`} games={backlog.slice(0, 12)} actions={actions} />
        {sameGenre.length > 1 && (
          <Shelf index={4} title={`Because you play ${genre}`} games={sameGenre.slice(0, 12)} actions={actions} />
        )}
        <Shelf index={5} title="Rediscover" note="Not played in a month" wide games={rediscover.slice(0, 8)} actions={actions} />
      </div>
    </div>
  );
}
