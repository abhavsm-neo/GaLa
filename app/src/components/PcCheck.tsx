import { listen } from '@tauri-apps/api/event';
import { createContext, useContext, useEffect, useState } from 'react';
import { api, type PcCheck as Check } from '../api';

export const VERDICT_LABEL: Record<string, string> = {
  well: 'Runs it well',
  lower: 'Lower settings',
  below: 'Below minimum',
  unknown: "Can't tell",
};
const BADGE: Record<string, string> = { well: 'Runs well', lower: 'Lower settings', below: 'Too weak' };

/** Verdicts for library games, for the small badge on covers. */
export const VerdictContext = createContext<Map<number, [string, string]>>(new Map());

export function useVerdicts() {
  const [verdicts, setVerdicts] = useState(new Map<number, [string, string]>());
  useEffect(() => {
    const load = () => api.pcVerdicts().then(list => setVerdicts(new Map(list.map(([id, verdict, summary]) => [id, [verdict, summary]]))));
    load();
    const stops = [listen('requirements-changed', load), listen('library-changed', load)];
    return () => stops.forEach(stop => stop.then(fn => fn()));
  }, []);
  return verdicts;
}

export function VerdictBadge({ gameId }: { gameId: number }) {
  const found = useContext(VerdictContext).get(gameId);
  if (!found || found[0] === 'unknown') return null;
  return <span className={`pcfit ${found[0]}`} title={found[1]}>{BADGE[found[0]]}</span>;
}

const tick = (meets: boolean | null) => (meets === null ? <i className="q">?</i> : meets ? <i className="y">✓</i> : <i className="n">✗</i>);

export function CheckTable({ check, compact }: { check: Check; compact?: boolean }) {
  const [full, setFull] = useState(false);
  return (
    <>
      <div className={`verdict ${check.verdict}`}>
        <b>{VERDICT_LABEL[check.verdict]}</b>
        <span>{check.summary}</span>
      </div>
      {!compact && (
        <table className="pctable">
          <thead><tr><th /><th>Yours</th><th>Min</th><th>Rec</th></tr></thead>
          <tbody>
            {check.rows.map(row => (
              <tr key={row.part}>
                <th>{row.part}</th>
                <td title={row.yours}>{row.yours}</td>
                <td title={row.minimum ?? ''}>{row.minimum === null ? '-' : tick(row.meets_minimum)}</td>
                <td title={row.recommended ?? ''}>{row.recommended === null ? '-' : tick(row.meets_recommended)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <button className="linkbtn" onClick={() => setFull(!full)}>{full ? 'Hide' : 'Show'} Steam's requirements</button>
      {full && (
        <div className="reqtext">
          {[['Minimum', check.minimum], ['Recommended', check.recommended]].filter(([, lines]) => lines.length).map(([label, lines]) => (
            <div key={label as string}>
              <small>{label as string}</small>
              {(lines as string[]).map(line => <p key={line}>{line}</p>)}
            </div>
          ))}
        </div>
      )}
      <p className="note">Estimated from {check.name || 'Steam'}'s listed requirements. Close calls between similar parts can go either way.</p>
    </>
  );
}

/** The game page panel. */
export function PcPanel({ gameId }: { gameId: number }) {
  const [check, setCheck] = useState<Check | null | undefined>(undefined);
  const [error, setError] = useState('');

  useEffect(() => {
    setCheck(undefined);
    setError('');
    api.pcCheck(gameId).then(setCheck).catch(e => setError(String(e)));
  }, [gameId]);

  return (
    <div className="panel">
      <h3>Can your PC run it?</h3>
      {error ? <p className="note">Couldn't reach Steam: {error}</p>
        : check === undefined ? <p className="note"><span className="pulse" /> Checking Steam's requirements</p>
          : check === null ? <p className="note">Steam doesn't list requirements for this game.</p>
            : <CheckTable check={check} />}
    </div>
  );
}
