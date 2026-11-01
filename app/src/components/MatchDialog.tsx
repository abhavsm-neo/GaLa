import { useEffect, useState } from 'react';
import { api, type Game, type Match } from '../api';

export function MatchDialog({ game, onClose }: { game: Game; onClose: () => void }) {
  const [query, setQuery] = useState(game.name);
  const [results, setResults] = useState<Match[] | null>(null);
  const [saving, setSaving] = useState<number | null>(null);
  const [error, setError] = useState('');

  const search = (text: string) => {
    setResults(null);
    setError('');
    api.searchMetadata(text).then(setResults).catch(e => { setResults([]); setError(String(e)); });
  };

  useEffect(() => search(game.name), [game.name]);

  const choose = async (match: Match) => {
    setSaving(match.igdb_id);
    setError('');
    try {
      await api.chooseMetadata(game.id, match.igdb_id);
      onClose();
    } catch (e) {
      setError(String(e));
      setSaving(null);
    }
  };

  return (
    <div className="overlay open">
      <div className="add" role="dialog" aria-label="Find this game">
        <h2>Which game is this?</h2>
        <form className="matchsearch" onSubmit={e => { e.preventDefault(); search(query); }}>
          <input className="field" aria-label="Game name" value={query} onChange={e => setQuery(e.target.value)} />
          <button className="btn sm" type="submit">Search</button>
        </form>
        <div className="matchlist">
          {results === null && <p className="note">Searching</p>}
          {results?.length === 0 && !error && <p className="note">Nothing found. Try a shorter name.</p>}
          {results?.map(match => (
            <button key={match.igdb_id} className="matchitem" disabled={saving !== null} onClick={() => choose(match)}>
              {match.cover ? <img src={match.cover} alt="" /> : <span className="nocover" />}
              <span>
                <b>{match.name}</b>
                <small>{[match.year, match.developer].filter(Boolean).join(' · ')}</small>
              </span>
              {saving === match.igdb_id && <small>Getting art</small>}
            </button>
          ))}
        </div>
        {error && <p className="err">{error}</p>}
        <div className="foot"><button className="btn sm" onClick={onClose}>Cancel</button></div>
      </div>
    </div>
  );
}
