import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { api, type Settings } from '../api';
import { ChooseFiles } from './FileTree';
import { CheckTable } from './PcCheck';
import type { PcCheck } from '../api';

type Props = {
  settings: Settings;
  initialFile: string;
  initialMagnet: string;
  onClose: () => void;
  onAdded: (settings: Settings) => void;
};

type Reply = { event: string; id?: string; message?: string; request?: string };

export function AddDialog({ settings, initialFile, initialMagnet, onClose, onAdded }: Props) {
  const [tab, setTab] = useState(initialFile ? 'File' : 'Magnet');
  const [magnet, setMagnet] = useState(initialMagnet);
  const [file, setFile] = useState(initialFile);
  const [folder, setFolder] = useState(settings.download_dir);
  const [error, setError] = useState('');
  const [request, setRequest] = useState('');
  const [held, setHeld] = useState('');
  const [saved, setSaved] = useState(settings);
  const [name, setName] = useState('');
  const [check, setCheck] = useState<PcCheck | null | undefined>(undefined);

  // The torrent's own name arrives with its first status, which for a magnet is after its file list.
  useEffect(() => {
    if (!held) return;
    const stop = listen<{ event: string; id?: string; name?: string; state?: string }>('engine-event', ({ payload }) => {
      if (payload.event === 'progress' && payload.id === held && payload.name && payload.name !== held && payload.state !== 'metadata') {
        setName(payload.name);
      }
    });
    return () => void stop.then(fn => fn());
  }, [held]);

  useEffect(() => {
    if (!name) return;
    let stale = false;
    api.pcCheckName(name).then(found => !stale && setCheck(found), () => !stale && setCheck(null));
    return () => { stale = true; };
  }, [name]);

  // The engine names the torrent once it's added, and the request ties that answer to this dialog.
  useEffect(() => {
    if (!request) return;
    const stop = listen<Reply>('engine-event', ({ payload }) => {
      if (payload.request !== request) return;
      if (payload.event === 'added' && payload.id) setHeld(payload.id);
      else {
        setError(payload.message ?? 'The torrent couldn\'t be added');
        setRequest('');
      }
    });
    return () => void stop.then(fn => fn());
  }, [request]);

  const pickFile = async () => {
    const picked = await open({ filters: [{ name: 'Torrent', extensions: ['torrent'] }] });
    if (picked) setFile(picked);
  };

  const pickFolder = async () => {
    const picked = await open({ directory: true, defaultPath: folder });
    if (picked) setFolder(picked);
  };

  const submit = async () => {
    if (tab === 'File' && !file) return setError('Choose a .torrent file first');
    const ask = crypto.randomUUID();
    setRequest(ask);
    setError('');
    try {
      if (tab === 'Magnet') await api.addMagnet(magnet.trim(), folder, ask);
      else await api.addTorrentFile(file, folder, ask);
      setSaved(await api.setDownloadDir(folder));
    } catch (e) {
      setError(String(e));
      setRequest('');
    }
  };

  const title = tab === 'File' ? file.split(/[\\/]/).pop() : 'Magnet link';

  return (
    <div className="overlay open">
      <div className={`add ${held ? 'wide' : ''}`} role="dialog" aria-label="Add a game">
        <h2>{held ? 'Choose files' : 'Add a game'}</h2>
        {held ? (
          <>
            <p className="note" style={{ marginTop: 0 }}>{name || title} · Saving to {folder}</p>
            {check && <div className="addcheck"><small>Can your PC run it?</small><CheckTable check={check} compact /></div>}
            <ChooseFiles
              id={held}
              savePath={folder}
              cancelLabel="Cancel"
              onStarted={() => onAdded(saved)}
              onCancel={() => {
                api.remove(held, false);
                onClose();
              }}
            />
          </>
        ) : (
          <>
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
              <button className="btn sm play" disabled={Boolean(request)} onClick={submit}>
                {request ? 'Adding' : 'Next: choose files'}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
