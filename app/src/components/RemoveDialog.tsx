import { useState } from 'react';
import { api, type Torrent } from '../api';

export function RemoveDialog({ torrent, onClose }: { torrent: Torrent; onClose: () => void }) {
  const [deleteFiles, setDeleteFiles] = useState(false);

  const confirm = () => {
    api.remove(torrent.id, deleteFiles);
    onClose();
  };

  return (
    <div className="overlay open">
      <div className="add" role="alertdialog" aria-label="Remove download">
        <h2>Remove {torrent.name}?</h2>
        <button className={`switch ${deleteFiles ? 'on' : ''}`} onClick={() => setDeleteFiles(!deleteFiles)}>
          Also delete the downloaded files<i />
        </button>
        <div className="foot">
          <button className="btn sm" onClick={onClose}>Keep it</button>
          <button className="btn sm danger" onClick={confirm}>Remove</button>
        </div>
      </div>
    </div>
  );
}
