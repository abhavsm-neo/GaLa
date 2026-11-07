import { useState } from 'react';
import { api, type Game } from '../api';

export function RemoveGameDialog({ game, onClose, onRemoved }: { game: Game; onClose: () => void; onRemoved: () => void }) {
  const [deleteFiles, setDeleteFiles] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const hasFiles = Boolean(game.exe_path) && !game.launcher && !game.install_state;

  const confirm = async () => {
    setBusy(true);
    try {
      await api.removeGame(game.id, deleteFiles);
      onRemoved();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="overlay open">
      <div className="add" role="alertdialog" aria-label="Remove game">
        <h2>Remove {game.name}?</h2>
        {hasFiles && (
          <button className={`switch ${deleteFiles ? 'on' : ''}`} onClick={() => setDeleteFiles(!deleteFiles)}>
            Also delete the game's files<i />
          </button>
        )}
        <p className="note">
          {deleteFiles
            ? `${game.install_dir} goes to the Recycle Bin. Windows asks first if it's too big to fit there.`
            : game.launcher
              ? `It stays installed in ${game.launcher}. Uninstall it there to free the space.`
              : 'Its files stay on disk.'}
        </p>
        {error && <p className="err">{error}</p>}
        <div className="foot">
          <button className="btn sm" onClick={onClose}>Keep it</button>
          <button className="btn sm danger" disabled={busy} onClick={confirm}>{busy ? 'Removing' : 'Remove'}</button>
        </div>
      </div>
    </div>
  );
}
