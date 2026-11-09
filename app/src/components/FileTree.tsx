import { listen } from '@tauri-apps/api/event';
import { useEffect, useMemo, useState } from 'react';
import { api, type TorrentFile } from '../api';
import { bytes } from '../format';

type Folder = { name: string; path: string; folders: Map<string, Folder>; files: number[]; all: number[]; size: number };

/** Folders keep the torrent's order. Padding files stay out of sight but keep their index. */
function buildTree(files: TorrentFile[]) {
  const root: Folder = { name: '', path: '', folders: new Map(), files: [], all: [], size: 0 };
  files.forEach((file, index) => {
    if (file.pad) return;
    const parts = file.path.split(/[\\/]/);
    let folder = root;
    folder.all.push(index);
    folder.size += file.size;
    for (const part of parts.slice(0, -1)) {
      const path = folder.path ? `${folder.path}/${part}` : part;
      if (!folder.folders.has(part)) folder.folders.set(part, { name: part, path, folders: new Map(), files: [], all: [], size: 0 });
      folder = folder.folders.get(part)!;
      folder.all.push(index);
      folder.size += file.size;
    }
    folder.files.push(index);
  });
  return root;
}

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

export function FileTree({ files, wanted, onChange, progress }: {
  files: TorrentFile[];
  wanted: boolean[];
  onChange: (next: boolean[]) => void;
  progress?: boolean;
}) {
  const root = useMemo(() => buildTree(files), [files]);
  const [closed, setClosed] = useState<Set<string>>(new Set());

  const set = (indices: number[], value: boolean) => {
    const next = [...wanted];
    for (const index of indices) next[index] = value;
    onChange(next);
  };

  const toggleOpen = (path: string) => setClosed(old => {
    const next = new Set(old);
    if (!next.delete(path)) next.add(path);
    return next;
  });

  const renderFolder = (folder: Folder, depth: number): React.ReactNode[] => [
    ...[...folder.folders.values()].flatMap(child => {
      const chosen = child.all.filter(i => wanted[i]).length;
      const open = !closed.has(child.path);
      return [
        <div className="frow folder" key={`d:${child.path}`} style={{ '--d': depth } as React.CSSProperties}>
          <button className="caret" aria-label={open ? `Collapse ${child.name}` : `Expand ${child.name}`} onClick={() => toggleOpen(child.path)}>
            {open ? '▾' : '▸'}
          </button>
          <input
            type="checkbox"
            aria-label={`Download ${child.name}`}
            checked={chosen === child.all.length}
            ref={box => { if (box) box.indeterminate = chosen > 0 && chosen < child.all.length; }}
            onChange={e => set(child.all, e.target.checked)}
          />
          <span className="fname">{child.name}</span>
          <span className="fsize">{bytes(child.size)}</span>
        </div>,
        ...(open ? renderFolder(child, depth + 1) : []),
      ];
    }),
    ...folder.files.map(index => {
      const file = files[index];
      const done = file.size ? Math.floor((file.done / file.size) * 100) : 100;
      return (
        <label className={`frow ${wanted[index] ? '' : 'off'}`} key={`f:${index}`} style={{ '--d': depth } as React.CSSProperties}>
          <span className="caret" />
          <input type="checkbox" checked={wanted[index]} onChange={e => set([index], e.target.checked)} />
          <span className="fname" title={file.path}>{fileName(file.path)}</span>
          {progress && <span className="fdone">{wanted[index] || done ? `${done}%` : 'Skipped'}</span>}
          <span className="fsize">{bytes(file.size)}</span>
        </label>
      );
    }),
  ];

  return <div className="ftree">{renderFolder(root, 0)}</div>;
}

export const chosenBytes = (files: TorrentFile[], wanted: boolean[]) =>
  files.reduce((sum, file, i) => sum + (wanted[i] && !file.pad ? file.size : 0), 0);

/** Waits for a held torrent's file list, then lets the files be picked before it starts. */
export function ChooseFiles({ id, savePath, cancelLabel, onStarted, onCancel }: {
  id: string;
  savePath: string;
  cancelLabel: string;
  onStarted: () => void;
  onCancel: () => void;
}) {
  const [files, setFiles] = useState<TorrentFile[] | null>(null);
  const [wanted, setWanted] = useState<boolean[]>([]);
  const [free, setFree] = useState<number | null>(null);

  useEffect(() => {
    api.freeSpace(savePath).then(setFree);
  }, [savePath]);

  useEffect(() => {
    if (files) return;
    let stale = false;
    const ask = () => api.detail(id);
    ask();
    const timer = setInterval(ask, 1500);
    const stop = listen<{ event: string; id?: string; files?: TorrentFile[] }>('engine-event', ({ payload }) => {
      if (stale || payload.event !== 'detail' || payload.id !== id || !payload.files?.length) return;
      setFiles(payload.files);
      setWanted(payload.files.map(file => file.wanted));
    });
    return () => {
      stale = true;
      clearInterval(timer);
      stop.then(fn => fn());
    };
  }, [id, files]);

  if (!files) {
    return (
      <>
        <p className="note"><span className="pulse" /> Getting the file list. Magnet links can take a moment to find peers.</p>
        <div className="foot"><button className="btn sm" onClick={onCancel}>{cancelLabel}</button></div>
      </>
    );
  }

  const visible = files.map((file, i) => (file.pad ? -1 : i)).filter(i => i >= 0);
  const count = visible.filter(i => wanted[i]).length;
  const size = chosenBytes(files, wanted);
  const total = chosenBytes(files, files.map(() => true));
  const short = free !== null && size > free;

  return (
    <>
      <div className="fbar">
        <span><b>{count}</b> of {visible.length} files · <b>{bytes(size)}</b> of {bytes(total)}</span>
        <button className="btn sm" onClick={() => setWanted(files.map(() => true))}>All</button>
        <button className="btn sm" onClick={() => setWanted(files.map(() => false))}>None</button>
      </div>
      <FileTree files={files} wanted={wanted} onChange={setWanted} />
      <p className={`note ${short ? 'warnnote' : ''}`}>
        {free === null ? '' : short ? `Only ${bytes(free)} free on this drive.` : `${bytes(free)} free on this drive.`}
      </p>
      <div className="foot">
        <button className="btn sm" onClick={onCancel}>{cancelLabel}</button>
        <button
          className="btn sm play"
          disabled={!count}
          onClick={async () => {
            await api.setFiles(id, wanted);
            onStarted();
          }}
        >
          Download {count === visible.length ? 'everything' : `${count} ${count === 1 ? 'file' : 'files'}`}
        </button>
      </div>
    </>
  );
}
