import { open } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
import { api, type Settings } from '../api';

type Props = {
  settings: Settings;
  initialFile: string;
  initialMagnet: string;
  onClose: () => void;
  onAdded: (settings: Settings) => void;
};

export function AddDialog({ settings, initialFile, initialMagnet, onClose, onAdded }: Props) {
  const [tab, setTab] = useState(initialFile ? 'File' : 'Magnet');
  const [magnet, setMagnet] = useState(initialMagnet);
  const [file, setFile] = useState(initialFile);
  const [folder, setFolder] = useState(settings.download_dir);
  const [error, setError] = useState('');

  const pickFile = async () => {
    const picked = await open({ filters: [{ name: 'Torrent', extensions: ['torrent'] }] });
    if (picked) setFile(picked);
  };

  const pickFolder = async () => {
    const picked = await open({ directory: true, defaultPath: folder });
    if (picked) setFolder(picked);
  };

  const submit = async () => {
    try {
      if (tab === 'Magnet') {
        await api.addMagnet(magnet.trim(), folder);
      } else {
        if (!file) return setError('Choose a .torrent file first');
        await api.addTorrentFile(file, folder);
      }
      onAdded(await api.setDownloadDir(folder));
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="overlay open">
      <div className="add" role="dialog" aria-label="Add a game">
        <h2>Add a game</h2>
        <div className="subt">
          {['Magnet', 'File'].map(name => (
            <button key={name} className={tab === name ? 'on' : ''} onClick={() => { setTab(name); setError(''); }}>
              {name === 'File' ? 'Torrent file' : 'Magnet link'}
            </button>
          ))}
        </div>
        {tab === 'Magnet' ? (
          <textarea
            autoFocus
            aria-label="Magnet link"
            placeholder="Paste a magnet link"
            value={magnet}
            onChange={e => { setMagnet(e.target.value); setError(''); }}
          />
        ) : (
          <button className="drop" onClick={pickFile}>{file || 'Choose a .torrent file'}</button>
        )}
        <div className="saveto">
          <span>Download to</span>
          <code>{folder}</code>
          <button className="btn sm" onClick={pickFolder}>Change</button>
        </div>
        <div className="err">{error}</div>
        <div className="foot">
          <button className="btn sm" onClick={onClose}>Cancel</button>
          <button className="btn sm play" onClick={submit}>Add download</button>
        </div>
      </div>
    </div>
  );
}
