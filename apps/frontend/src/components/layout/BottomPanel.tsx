/**
 * BottomPanel — resizable bottom area with tabbed content.
 */
import type { ReactNode } from 'react';
import { useRef, useCallback, useEffect } from 'react';
import { useLayoutStore } from '../../stores/layoutStore';
import { PanelTabBar } from './PanelTabBar';

interface Props {
  children: ReactNode;
}

export function BottomPanel({ children }: Props) {
  const visible = useLayoutStore((s) => s.bottomPanelVisible);
  const height = useLayoutStore((s) => s.bottomPanelHeight);
  const setHeight = useLayoutStore((s) => s.setBottomPanelHeight);
  const resizing = useRef(false);
  const startY = useRef(0);
  const startH = useRef(0);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    resizing.current = true;
    startY.current = e.clientY;
    startH.current = height;
  }, [height]);

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!resizing.current) return;
      const delta = startY.current - e.clientY;
      setHeight(startH.current + delta);
    };
    const onMouseUp = () => {
      resizing.current = false;
    };
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    return () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };
  }, [setHeight]);

  if (!visible) return null;

  return (
    <div className="flex-shrink-0 border-t border-border flex flex-col" style={{ height }}>
      {/* Resize handle (top edge) */}
      <div
        onMouseDown={onMouseDown}
        className="h-1 cursor-row-resize hover:bg-accent/30 transition-colors flex-shrink-0"
      />
      <PanelTabBar />
      <div className="flex-1 overflow-hidden bg-bg-primary">
        {children}
      </div>
    </div>
  );
}
