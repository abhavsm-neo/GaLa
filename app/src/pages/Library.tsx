import { open } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
import { api, type Game } from '../api';
import { Banner, Capsule, GameArt, type GameActions } from '../components/GameCards';
import { lastPlayed, playtime } from '../format';

const COLLECTIONS: [string, (g: Game) => boolean][] = [
  ['All games', () => true],
  ['Favourites', g => g.favorite],
  ['Ready to play', g => Boolean(g.exe_path)],
  ['Needs setup', g => !g.exe_path && !g.install_state],
  ['To install', g => Boolean(g.install_state)],
  ['Playing', g => g.progress === 'Playing'],
  ['Backlog', g => g.progress === 'Backlog'],
  ['Finished', g => g.progress === 'Finished'],
  ['Dropped', g => g.progress === 'Dropped'],
];

const SORTS: Record<string, (a: Game, b: Game) => number> = {
  recent: (a, b) => (b.last_played ?? 0) - (a.last_played ?? 0) || a.name.localeCompare(b.name),
  name: (a, b) => a.name.localeCompare(b.name),
  hours: (a, b) => b.total_seconds - a.total_seconds,
  added: (a, b) => b.added_at - a.added_at,
  released: (a, b) => (b.release_date ?? 0) - (a.release_date ?? 0),
  rated: (a, b) => (b.igdb_rating ?? 0) - (a.igdb_rating ?? 0),
};

const unique = (values: string[]) => [...new Set(values)].sort((a, b) => a.localeCompare(b));
const searchText = (g: Game) => [g.name, g.developer, g.publisher, ...g.genres, ...g.categories].join(' ').toLowerCase();
const year = (g: Game) => (g.release_date ? new Date(g.release_date * 1000).getUTCFullYear() : '-');

export function Library({ games, actions }: { games: Game[]; actions: GameActions }) {
  const [collection, setCollection] = useState('All games');
  const [sort, setSort] = useState('recent');
  const [mode, setMode] = useState('grid');
  const [size, setSize] = useState(178);
  const [search, setSearch] = useState('');

  const genres = unique(games.flatMap(g => g.genres));
  const categories = unique(games.flatMap(g => g.categories));
  const inGenre = (name: string) => (g: Game) => g.genres.includes(name);
  const inCategory = (name: string) => (g: Game) => g.categories.includes(name);
  const filters: [string, (g: Game) => boolean][] = [
    ...COLLECTIONS,
    ...genres.map((name): [string, (g: Game) => boolean] => [`genre:${name}`, inGenre(name)]),
    ...categories.map((name): [string, (g: Game) => boolean] => [`category:${name}`, inCategory(name)]),
  ];
  // The chosen category can vanish when its last game drops it.
  const test = filters.find(([name]) => name === collection)?.[1] ?? (() => true);
  const list = games
    .filter(test)
    .filter(g => searchText(g).includes(search.toLowerCase()))
    .sort(SORTS[sort]);

  const sideItem = (key: string, label: string, filter: (g: Game) => boolean) => (
    <button key={key} className={`sideitem ${collection === key ? 'on' : ''}`} onClick={() => setCollection(key)}>
      {label}<span>{games.filter(filter).length}</span>
    </button>
  );

  const addExisting = async () => {
    const file = await open({ filters: [{ name: 'Game, installer or archive', extensions: ['exe', 'msi', 'iso', 'zip', '7z', 'rar'] }] });
    if (file) actions.open(await api.addGame(file));
  };

  return (
    <div className="libgrid">
      <aside className="libside">
        <div className="side-h">Collections</div>
        {COLLECTIONS.map(([name, filter]) => sideItem(name, name, filter))}
        {categories.length > 0 && <div className="side-h">Your categories</div>}
        {categories.map(name => sideItem(`category:${name}`, name, inCategory(name)))}
        {genres.length > 0 && <div className="side-h">Genres</div>}
        {genres.map(name => sideItem(`genre:${name}`, name, inGenre(name)))}
        <div className="side-h">All games</div>
        {[...games].sort(SORTS.name).map(g => (
          <button key={g.id} className="minigame" onClick={() => actions.open(g.id)}>
            <GameArt game={g} wide /><span>{g.name}</span>
          </button>
        ))}
      </aside>
      <div className="libmain" style={{ '--card': `${size}px` } as React.CSSProperties}>
        <div className="tools">
          <h1>{collection.replace(/^(genre|category):/, '')}<small>{list.length} games</small></h1>
          <input className="field libsearch" aria-label="Search your library" placeholder="Search" value={search} onChange={e => setSearch(e.target.value)} />
          <span className="spacer" />
          <label className="sizer">Sort
            <select className="select" value={sort} onChange={e => setSort(e.target.value)}>
              <option value="recent">Recently played</option>
              <option value="name">Name</option>
              <option value="hours">Playtime</option>
              <option value="added">Recently added</option>
              <option value="released">Release date</option>
              <option value="rated">Rating</option>
            </select>
          </label>
          <div className="seg">
            {[['grid', 'Covers'], ['wide', 'Banners'], ['list', 'Details']].map(([value, label]) => (
              <button key={value} className={mode === value ? 'on' : ''} onClick={() => setMode(value)}>{label}</button>
            ))}
          </div>
          {mode === 'grid' && (
            <label className="sizer">Size
              <input type="range" min="130" max="260" value={size} aria-label="Cover size" onChange={e => setSize(Number(e.target.value))} />
            </label>
          )}
          <button className="btn sm" onClick={addExisting}>Add existing game</button>
        </div>

        {!list.length && <p style={{ color: 'var(--t3)', padding: 40 }}>No games here yet.</p>}
        {mode === 'list' ? (
          <table className="tbl">
            <thead><tr><th /><th>Name</th><th>Genre</th><th>Year</th><th>Status</th><th>Played</th><th>Last played</th></tr></thead>
            <tbody>
              {list.map(g => (
                <tr key={g.id} onClick={() => actions.open(g.id)}>
                  <td><GameArt game={g} wide /></td>
                  <td><b>{g.name}</b></td>
                  <td>{g.genres[0] ?? '-'}</td>
                  <td>{year(g)}</td>
                  <td>{g.exe_path ? g.progress : 'Needs setup'}</td>
                  <td>{playtime(g.total_seconds)}</td>
                  <td>{lastPlayed(g.last_played)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <div className="grid">
            {list.map(g => mode === 'wide'
              ? <Banner key={g.id} game={g} actions={actions} />
              : <Capsule key={g.id} game={g} actions={actions} />)}
          </div>
        )}
      </div>
    </div>
  );
}
