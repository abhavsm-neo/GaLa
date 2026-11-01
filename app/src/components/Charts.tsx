import { useEffect, useRef } from 'react';
import type { Sample } from '../useEngine';

function prepare(canvas: HTMLCanvasElement) {
  const scale = window.devicePixelRatio || 1;
  const { width, height } = canvas.getBoundingClientRect();
  canvas.width = width * scale;
  canvas.height = height * scale;
  const context = canvas.getContext('2d')!;
  context.scale(scale, scale);
  return { context, width, height };
}

const token = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name);

export function NetGraph({ history }: { history: Sample[] }) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const { context, width, height } = prepare(ref.current!);
    const top = Math.max(1, ...history.flatMap(s => [s.down, s.up]));
    const line = (pick: (s: Sample) => number, color: string) => {
      context.beginPath();
      history.forEach((sample, i) => {
        const x = width - (history.length - 1 - i) * (width / 149);
        const y = height - 6 - (pick(sample) / top) * (height - 16);
        if (i) context.lineTo(x, y);
        else context.moveTo(x, y);
      });
      context.strokeStyle = color;
      context.lineWidth = 2;
      context.stroke();
    };
    line(s => s.up, token('--info'));
    line(s => s.down, token('--go'));
  }, [history]);

  return <canvas ref={ref} id="graph" aria-label="Network speed, last 2.5 minutes" />;
}

export function PieceMap({ pieces }: { pieces: number[] }) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const { context, width, height } = prepare(ref.current!);
    const cell = width / Math.max(1, pieces.length);
    context.fillStyle = 'rgba(255,255,255,.08)';
    context.fillRect(0, 0, width, height);
    context.fillStyle = token('--go');
    pieces.forEach((have, i) => {
      if (!have) return;
      context.globalAlpha = 0.25 + have * 0.75;
      context.fillRect(i * cell, 0, Math.ceil(cell), height);
    });
  }, [pieces]);

  return <canvas ref={ref} className="piecemap" aria-label="Downloaded pieces" />;
}
