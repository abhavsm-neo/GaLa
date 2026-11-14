import { convertFileSrc } from '@tauri-apps/api/core';
import { useEffect, useRef, useState } from 'react';
import { api, type Article, type Game } from '../api';
import { ago, tint } from '../format';

/** Fetches an article's picture once it scrolls into view, then shows the saved copy. */
function NewsImage({ article, game }: { article: Article; game?: Game }) {
  const [path, setPath] = useState(article.image);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (path || !article.has_image || !box.current) return;
    const watcher = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) return;
      watcher.disconnect();
      api.newsImage(article.id).then(setPath).catch(() => {});
    }, { rootMargin: '300px' });
    watcher.observe(box.current);
    return () => watcher.disconnect();
  }, [article.id, article.has_image, path]);

  const fallback = game?.hero ?? game?.cover;
  return (
    <div className="nimg" ref={box} style={{ '--c': tint(article.source.length * 97) } as React.CSSProperties}>
      {path ? <img src={convertFileSrc(path)} alt="" /> : fallback ? <img src={convertFileSrc(fallback)} alt="" /> : <span>{article.source}</span>}
    </div>
  );
}

export function openArticle(article: Article, markRead: (ids: number[]) => void) {
  api.openUrl(article.url);
  if (!article.read) markRead([article.id]);
}

export function NewsCard({ article, games, markRead }: { article: Article; games: Game[]; markRead: (ids: number[]) => void }) {
  const [first] = games;
  return (
    <article className={`ncard ${article.read ? 'read' : ''}`} tabIndex={0} onClick={() => openArticle(article, markRead)} onKeyDown={e => e.key === 'Enter' && openArticle(article, markRead)}>
      <NewsImage article={article} game={first} />
      <div className="nbody">
        <small>
          {!article.read && <i className="unread" aria-label="Unread" />}
          {article.source} · {ago(article.published)}
          {first && <span className="ngame">{first.name}</span>}
        </small>
        <b>{article.title}</b>
        {article.summary && <p>{article.summary}</p>}
      </div>
    </article>
  );
}

/** A compact row for Home and game pages. */
export function NewsRow({ article, games, markRead }: { article: Article; games: Game[]; markRead: (ids: number[]) => void }) {
  return (
    <button className={`nrow ${article.read ? 'read' : ''}`} onClick={() => openArticle(article, markRead)}>
      <NewsImage article={article} game={games[0]} />
      <span>
        <b>{article.title}</b>
        <small>{!article.read && <i className="unread" aria-label="Unread" />}{article.source} · {ago(article.published)}</small>
      </span>
    </button>
  );
}
