import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { api, type Game } from '../api';
import { bytes } from '../format';

const DRIVES = [['E drive', 'E:\\GaLa\\Games'], ['C drive', 'C:\\GaLa\\Games']];

export function InstallDialog({ game, folder: initial, onClose, onStart }: {
  game: Game;
  folder: string;
  onClose: () => void;
  onStart: (folder: string, interactive: boolean, password: string) => void;
}) {
  const [folder, setFolder] = useState(initial);
  const [password, setPassword] = useState('');
  const locked = game.install_error.toLowerCase().includes('password');
  const [free, setFree] = useState<number | null>(null);

  useEffect(() => {
    api.freeSpace(folder).then(setFree);
  }, [folder]);

  const browse = async () => {
    const picked = await open({ directory: true, defaultPath: folder });
    if (picked) setFolder(picked);
  };

  return (
    <div className="overlay open">
      <div className="add" role="dialog" aria-label="Install game">
        <h2>Install {game.name}</h2>
        <div className="subt">
          {DRIVES.map(([label, path]) => (
            <button key={path} className={folder === path ? 'on' : ''} onClick={() => setFolder(path)}>{label}</button>
          ))}
        </div>
        <div className="saveto">
          <span>Install to</span>
          <code>{folder}</code>
          <button className="btn sm" onClick={browse}>Change</button>
        </div>
        {locked && (
          <input
            autoFocus
            className="field"
            style={{ marginTop: 12 }}
            aria-label="Archive password"
            placeholder="Archive password"
            value={password}
            onChange={e => setPassword(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && password && onStart(folder, false, password)}
          />
        )}
        {locked && <p className="note">Downloads often name the password in a text file next to the archive, or in the torrent's description.</p>}
        <p className="note">{free === null ? '' : `${bytes(free)} free here. `}Installers may show a Windows permission prompt.</p>
        <div className="foot">
          <button className="btn sm" onClick={onClose}>Cancel</button>
          <button className="btn sm" onClick={() => onStart(folder, true, password)}>Use the game's own installer</button>
          <button className="btn sm play" disabled={locked && !password} onClick={() => onStart(folder, false, password)}>Install</button>
        </div>
      </div>
    </div>
  );
}
