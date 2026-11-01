import { useEffect, useState } from 'react';
import { api, type DayPlaytime, type Game } from '../api';
import { GameArt, type GameActions } from '../components/GameCards';
import { playtime } from '../format';

const HEAT_DAYS = 371;
const DAY = 86400000;

const dayKey = (date: Date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;

function longestStreak(played: Set<string>) {
  let best = 0;
  let run = 0;
  for (let i = HEAT_DAYS - 1; i >= 0; i--) {
    run = played.has(dayKey(new Date(Date.now() - i * DAY))) ? run + 1 : 0;
    best = Math.max(best, run);
  }
  return best;
}

export function Stats({ games, actions }: { games: Game[]; actions: GameActions }) {
  const [days, setDays] = useState<DayPlaytime[]>([]);
  const total = games.reduce((sum, g) => sum + g.total_seconds, 0);

  useEffect(() => {
    api.playtimeByDay(null, HEAT_DAYS).then(setDays);
  }, [total]);

  const byDay = new Map(days.map(d => [d.day, d.seconds]));
  const top = Math.max(1, ...days.map(d => d.seconds));
  const level = (seconds: number) => (seconds ? Math.min(4, Math.ceil((seconds / top) * 4)) : 0);
  const cells = Array.from({ length: HEAT_DAYS }, (_, i) =>
    byDay.get(dayKey(new Date(Date.now() - (HEAT_DAYS - 1 - i) * DAY))) ?? 0);

  const played = games.filter(g => g.total_seconds);
  const most = [...played].sort((a, b) => b.total_seconds - a.total_seconds).slice(0, 7);
  const finished = games.filter(g => g.progress === 'Finished').length;
  const streak = longestStreak(new Set(byDay.keys()));

  return (
    <div className="page">
      <h1 style={{ fontSize: 40, marginBottom: 14 }}>Your gaming year</h1>
      <div className="tiles">
        <div className="tile"><span>Total playtime</span><b>{playtime(total)}</b></div>
        <div className="tile"><span>Games played</span><b>{played.length} of {games.length}</b><small>{finished} finished</small></div>
        <div className="tile"><span>Longest streak</span><b>{streak} {streak === 1 ? 'day' : 'days'}</b><small>Past year</small></div>
      </div>
      <div className="statgrid">
        <div className="panel">
          <h3>Play activity</h3>
          <div className="heat" aria-label="Daily play activity">
            {cells.map((seconds, i) => <i key={i} className={`l${level(seconds)}`} title={playtime(seconds)} />)}
          </div>
        </div>
        <div className="panel">
          <h3>Most played</h3>
          {!most.length && <p>Nothing played yet.</p>}
          {most.map(g => (
            <div className="topg" key={g.id} onClick={() => actions.open(g.id)}>
              <GameArt game={g} />
              <div>
                <b>{g.name}</b>
                <div className="tinybar"><i style={{ width: `${(g.total_seconds / most[0].total_seconds) * 100}%` }} /></div>
              </div>
              <span style={{ textAlign: 'right', color: 'var(--t2)' }}>{playtime(g.total_seconds)}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
