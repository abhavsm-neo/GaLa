import type { Game, Running } from '../api';
import { Banner, Capsule, GameArt, PlayBox, type GameActions } from '../components/GameCards';
import { lastPlayed, playtime } from '../format';

type Props = { games: Game[]; running: Running | null; actions: GameActions; goLibrary: () => void };

export function Home({ games, running, actions, goLibrary }: Props) {
  if (!games.length) {
    return (
      <div className="empty">
        <h2>Your library is empty</h2>
        <p>Games appear here once a download finishes, or add one you already have.</p>
        <button className="btn play" onClick={goLibrary}>Go to library</button>
      </div>
    );
  }

  const played = games.filter(g => g.last_played).sort((a, b) => b.last_played! - a.last_played!);
  const added = [...games].sort((a, b) => b.added_at - a.added_at);
  const backlog = games.filter(g => g.exe_path && !g.total_seconds);
  const hero = played[0] ?? added[0];

  return (
    <>
      <section className="ghero homehero">
        <GameArt game={hero} wide />
        <div className="herotext">
          <div className="kicker">{played[0] ? 'Continue playing' : 'New in your library'}</div>
          <h1>{hero.name}</h1>
          <div className="meta">
            <span><b>{playtime(hero.total_seconds)}</b></span>
            <span>Last played {lastPlayed(hero.last_played).toLowerCase()}</span>
          </div>
          <div className="herobtns">
            {hero.exe_path && <PlayBox game={hero} running={running} actions={actions} />}
            <button className="btn" style={{ height: 52 }} onClick={() => actions.open(hero.id)}>Game details</button>
          </div>
        </div>
      </section>
      <div className="page" style={{ paddingTop: 0 }}>
        {played.length > 0 && (
          <section className="shelf">
            <div className="shelfhead"><h2>Jump back in</h2><span>Your recent sessions</span></div>
            <div className="row" style={{ '--w': '310px' } as React.CSSProperties}>
              {played.slice(0, 6).map(g => <Banner key={g.id} game={g} actions={actions} />)}
            </div>
          </section>
        )}
        <section className="shelf">
          <div className="shelfhead"><h2>New in your library</h2></div>
          <div className="row">{added.slice(0, 8).map(g => <Capsule key={g.id} game={g} actions={actions} />)}</div>
        </section>
        {backlog.length > 0 && (
          <section className="shelf">
            <div className="shelfhead"><h2>Clear the backlog</h2><span>{backlog.length} waiting</span></div>
            <div className="row" style={{ '--w': '310px' } as React.CSSProperties}>
              {backlog.map(g => <Banner key={g.id} game={g} actions={actions} />)}
            </div>
          </section>
        )}
      </div>
    </>
  );
}
