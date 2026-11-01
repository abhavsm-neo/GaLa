import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { api, type Game } from '../api';
import { bytes } from '../format';

const DRIVES = [['E drive', 'E:\\GaLa\\Games'], ['C drive', 'C:\\GaLa\\Games']];

export function InstallDialog({ game, folder: initial, onClose, onStart }: {
  game: Game;
  folder: string;
  onClose: () => void;
  onStart: (folder: string, interactive: boolean) => void;
}) {
  const [folder, setFolder] = useState(initial);
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
        <p className="note">{free === null ? '' : `${bytes(free)} free here. `}Installers may show a Windows permission prompt.</p>
        <div className="foot">
          <button className="btn sm" onClick={onClose}>Cancel</button>
          <button className="btn sm" onClick={() => onStart(folder, true)}>Use the game's own installer</button>
          <button className="btn sm play" onClick={() => onStart(folder, false)}>Install</button>
        </div>
      </div>
    </div>
  );
}
