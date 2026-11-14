import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useState } from 'react';
import { api, type Article, type Game } from './api';

export function useNews() {
  const [articles, setArticles] = useState<Article[]>([]);

  const load = useCallback(() => api.newsList().then(setArticles), []);

  useEffect(() => {
    load();
    const stop = listen('news-changed', load);
    return () => void stop.then(fn => fn());
  }, [load]);

  const markRead = useCallback((ids: number[]) => {
    setArticles(all => all.map(a => (ids.includes(a.id) ? { ...a, read: true } : a)));
    api.newsMarkRead(ids);
  }, []);

  const markAllRead = useCallback(() => {
    setArticles(all => all.map(a => ({ ...a, read: true })));
    api.newsMarkAllRead();
  }, []);

  return { articles, reload: load, markRead, markAllRead };
}

// One-word titles that are everyday words would match half the headlines.
const COMMON = new Set([
  'control', 'prey', 'inside', 'journey', 'rust', 'portal', 'ready', 'anthem', 'destiny', 'limbo', 'among', 'spore',
  'fable', 'thief', 'brink', 'evolve', 'payday', 'scum', 'raft', 'grounded', 'stray', 'deliver', 'blur', 'echo',
]);
const GENERIC_PHRASES = new Set(['the finals', 'the forest', 'the witness', 'the long dark', 'the crew']);

const normalise = (text: string) => text.toLowerCase().replace(/[™®©]/g, '').replace(/[^\p{L}\p{N}]+/gu, ' ').trim();

/** Which library games an article is about. Steam posts name their game; headlines are matched by whole title. */
export function gamesIn(article: Article, games: Game[]): Game[] {
  if (article.game_id) return games.filter(g => g.id === article.game_id);
  const title = ` ${normalise(article.title)} `;
  return games.filter(game => {
    const name = normalise(game.name);
    if (name.length < 4) return false;
    const single = !name.includes(' ');
    if (single && COMMON.has(name)) return false;
    // Phrases that read as ordinary English need the game's exact capitalisation in the headline.
    if (GENERIC_PHRASES.has(name)) return article.title.includes(game.name);
    return title.includes(` ${name} `);
  });
}
