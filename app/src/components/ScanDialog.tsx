import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { api, type Found } from '../api';

const SUGGESTED = ['C:\\Games', 'E:\\Games'];
const COVER_GAP = 250;

const describe = (game: Found) => {
  if (game.added) return 'Already in your library';
  if (game.launcher) return `Opens in ${game.launcher}`;
  if (game.installable) return 'Needs installing';
  return game.sure ? game.folder : `Check this one: ${game.folder}`;
};

export function ScanDialog({ onClose }: { onClose: () => void }) {
  const [folders, setFolders] = useState<string[]>([]);
  const [found, setFound] = useState<Found[] | null>(null);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [covers, setCovers] = useState<Record<string, string>>({});
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setFound(null);
    let stale = false;
    api.scanGames(folders, true)
      .then(games => {
        if (stale) return;
        setFound(games);
        setChosen(old => new Set([...old, ...games.filter(g => g.sure && !g.added).map(g => g.folder)]));
      })
      .catch(e => { setFound([]); setError(String(e)); });
    return () => { stale = true; };
  }, [folders]);

  useEffect(() => {
    if (!found) return;
    let stale = false;
    (async () => {
      for (const game of found) {
        if (stale) return;
        if (covers[game.folder] || game.added) continue;
        const cover = await api.searchMetadata(game.name).then(matches => matches[0]?.cover, () => undefined);
        if (cover && !stale) setCovers(old => ({ ...old, [game.folder]: cover }));
        await new Promise(resolve => setTimeout(resolve, COVER_GAP));
      }
    })();
    return () => { stale = true; };
  }, [found]);

  const addFolder = (folder: string) => setFolders(old => (old.includes(folder) ? old : [...old, folder]));

  const browse = async () => {
    const folder = await open({ directory: true });
    if (folder) addFolder(folder);
  };

  const toggle = (folder: string) => setChosen(old => {
    const next = new Set(old);
    if (!next.delete(folder)) next.add(folder);
    return next;
  });

  const picked = found?.filter(game => chosen.has(game.folder) && !game.added) ?? [];

  const add = async () => {
    setSaving(true);
    try {
      await api.addScanned(picked);
      onClose();
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  return (
    <div className="overlay open">
      <div className="add scanbox" role="dialog" aria-label="Find games on this PC">
        <h2>Find games on this PC</h2>
        <div className="chips">
          {SUGGESTED.filter(folder => !folders.includes(folder)).map(folder => (
            <button key={folder} className="chip" onClick={() => addFolder(folder)}>+ {folder}</button>
          ))}
          <button className="chip" onClick={browse}>+ Choose a folder</button>
        </div>
        {folders.length > 0 && <p className="note">Also looking in {folders.join(', ')}</p>}
        <div className="matchlist">
          {found === null && <p className="note">Looking for games</p>}
          {found?.length === 0 && !error && <p className="note">No games found. Try choosing a folder.</p>}
          {found?.map(game => (
            <label key={game.folder} className={`scanitem ${game.added ? 'off' : ''}`}>
              <input
                type="checkbox"
                aria-label={`Add ${game.name}`}
                disabled={game.added}
                checked={chosen.has(game.folder) && !game.added}
                onChange={() => toggle(game.folder)}
              />
              {covers[game.folder] ? <img src={covers[game.folder]} alt="" /> : <span className="nocover" />}
              <span>
                <b>{game.name}</b>
                <small title={game.folder}>{describe(game)}</small>
              </span>
            </label>
          ))}
        </div>
        {error && <p className="err">{error}</p>}
        <div className="foot">
          <button className="btn sm" onClick={onClose}>Cancel</button>
          <button className="btn sm play" disabled={!picked.length || saving} onClick={add}>
            Add {picked.length} {picked.length === 1 ? 'game' : 'games'}
          </button>
        </div>
      </div>
    </div>
  );
}
