import { useMemo, useState } from 'react';
import { api, type Article, type Game, type NewsFeed, type Settings } from '../api';
import { NewsCard } from '../components/NewsCards';
import { gamesIn } from '../useNews';

const PAGE = 36;
const YOURS = 'Your games';
const STEAM = 'Steam';

function FeedsDialog({ settings, onSaved, onClose }: { settings: Settings; onSaved: (settings: Settings) => void; onClose: () => void }) {
  const [feeds, setFeeds] = useState<NewsFeed[]>(settings.news_feeds);
  const [url, setUrl] = useState('');
  const [name, setName] = useState('');
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState('');

  const check = async () => {
    setChecking(true);
    setError('');
    try {
      const title = await api.checkNewsFeed(url);
      setName(old => old || title);
    } catch (e) {
      setError(String(e));
    } finally {
      setChecking(false);
    }
  };

  const add = () => {
    const clean = name.trim();
    if (!clean || feeds.some(feed => feed.name === clean || feed.url === url.trim())) {
      return setError('That feed or name is already in the list');
    }
    setFeeds([...feeds, { name: clean, url: url.trim() }]);
    setUrl('');
    setName('');
  };

  return (
    <div className="overlay open">
      <div className="add" role="dialog" aria-label="News feeds">
        <h2>News feeds</h2>
        <div className="feedlist">
          {feeds.map(feed => (
            <div className="kv" key={feed.url}>
              <span title={feed.url}>{feed.name}</span>
              <button className="btn sm" aria-label={`Remove ${feed.name}`} onClick={() => setFeeds(feeds.filter(f => f !== feed))}>Remove</button>
            </div>
          ))}
          {!feeds.length && <p className="note">No feeds. Steam news for your Steam games still comes in.</p>}
        </div>
        <h3 style={{ marginTop: 16 }}>Add a feed</h3>
        <div className="feedadd">
          <input className="field" placeholder="https://example.com/feed" aria-label="Feed address" value={url} onChange={e => { setUrl(e.target.value); setName(''); setError(''); }} />
          <button className="btn sm" disabled={!url.trim() || checking} onClick={check}>{checking ? 'Checking' : 'Check'}</button>
        </div>
        {name && (
          <div className="feedadd">
            <input className="field" aria-label="Feed name" value={name} onChange={e => setName(e.target.value)} />
            <button className="btn sm play" onClick={add}>Add</button>
          </div>
        )}
        <div className="err">{error}</div>
        <p className="note">Removing a feed also removes its articles.</p>
        <div className="foot">
          <button className="btn sm" onClick={onClose}>Cancel</button>
          <button className="btn sm play" onClick={async () => { onSaved(await api.setNewsFeeds(feeds)); onClose(); }}>Save</button>
        </div>
      </div>
    </div>
  );
}

export function News({ articles, games, settings, markRead, markAllRead, onSettings }: {
  articles: Article[];
  games: Game[];
  settings: Settings | null;
  markRead: (ids: number[]) => void;
  markAllRead: () => void;
  onSettings: (settings: Settings) => void;
}) {
  const [source, setSource] = useState('All');
  const [query, setQuery] = useState('');
  const [shown, setShown] = useState(PAGE);
  const [editing, setEditing] = useState(false);

  const matched = useMemo(() => new Map(articles.map(a => [a.id, gamesIn(a, games)])), [articles, games]);
  const sources = useMemo(() => [...new Set(articles.map(a => a.source))].sort((a, b) => (a === STEAM ? -1 : b === STEAM ? 1 : a.localeCompare(b))), [articles]);
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const list = articles.filter(a =>
    (source === 'All' || (source === YOURS ? matched.get(a.id)!.length > 0 : a.source === source))
    && words.every(word => `${a.title} ${a.summary}`.toLowerCase().includes(word)));
  const unread = articles.filter(a => !a.read).length;
  const yours = articles.filter(a => matched.get(a.id)!.length > 0).length;

  const pick = (name: string) => { setSource(name); setShown(PAGE); };

  return (
    <div className="page newspage">
      <div className="newshead">
        <h1>News<small>{unread ? `${unread} unread` : 'All caught up'}</small></h1>
        <input className="field libsearch" placeholder="Search news" aria-label="Search news" value={query} onChange={e => { setQuery(e.target.value); setShown(PAGE); }} />
        <span className="spacer" />
        <button className="btn sm" onClick={() => api.newsRefresh()}>Check now</button>
        <button className="btn sm" disabled={!unread} onClick={markAllRead}>Mark all read</button>
        <button className="btn sm" disabled={!settings} onClick={() => setEditing(true)}>Feeds</button>
      </div>
      <div className="chips newschips">
        {['All', YOURS, ...sources].map(name => (
          <button key={name} className={`chip ${source === name ? 'on' : ''}`} onClick={() => pick(name)}>
            {name}{name === YOURS ? ` (${yours})` : ''}
          </button>
        ))}
      </div>
      {!articles.length && <p className="note">No news yet. GaLa checks the feeds when it starts and every hour after.</p>}
      {articles.length > 0 && !list.length && <p className="note">Nothing matches.{source === YOURS ? ' No recent articles mention games in your library.' : ''}</p>}
      <div className="ngrid">
        {list.slice(0, shown).map(a => <NewsCard key={a.id} article={a} games={matched.get(a.id)!} markRead={markRead} />)}
      </div>
      {list.length > shown && (
        <div style={{ textAlign: 'center', marginTop: 18 }}>
          <button className="btn" onClick={() => setShown(shown + PAGE)}>Show more ({list.length - shown} left)</button>
        </div>
      )}
      {editing && settings && <FeedsDialog settings={settings} onSaved={onSettings} onClose={() => setEditing(false)} />}
    </div>
  );
}
