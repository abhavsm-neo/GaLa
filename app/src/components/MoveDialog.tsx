import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { api, type Game } from '../api';
import { bytes } from '../format';

const DRIVES = [['E drive', 'E:\\GaLa\\Games'], ['C drive', 'C:\\GaLa\\Games']];

export function MoveDialog({ game, size, onClose }: { game: Game; size: number | null; onClose: () => void }) {
  const here = game.install_dir.slice(0, 2).toUpperCase();
  const [folder, setFolder] = useState(DRIVES.find(([, path]) => !path.startsWith(here))?.[1] ?? DRIVES[0][1]);
  const [free, setFree] = useState<number | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    api.freeSpace(folder).then(setFree);
  }, [folder]);

  const browse = async () => {
    const picked = await open({ directory: true, defaultPath: folder });
    if (picked) setFolder(picked);
  };

  const start = async () => {
    setError('');
    try {
      await api.moveGame(game.id, folder);
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="overlay open">
      <div className="add" role="dialog" aria-label="Move game">
        <h2>Move {game.name}</h2>
        <div className="subt">
          {DRIVES.map(([label, path]) => (
            <button key={path} className={folder === path ? 'on' : ''} onClick={() => setFolder(path)}>{label}</button>
          ))}
        </div>
        <div className="saveto">
          <span>Move to</span>
          <code>{folder}</code>
          <button className="btn sm" onClick={browse}>Change</button>
        </div>
        <p className="note">
          Moves everything in {game.install_dir}{size === null ? '' : `, ${bytes(size)}`}.
          {free === null ? '' : ` ${bytes(free)} free there.`}
          {game.torrent_id && !game.source_path ? ' This game will stop seeding.' : ''}
          {' '}Games with their own installer may need repairing if they rely on their old location.
        </p>
        {error && <p className="err">{error}</p>}
        <div className="foot">
          <button className="btn sm" onClick={onClose}>Cancel</button>
          <button className="btn sm play" onClick={start}>Move</button>
        </div>
      </div>
    </div>
  );
}
