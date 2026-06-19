/**
 * SecondarySidebar — content area that switches based on ActivityBar selection.
 * Includes a resizable right edge.
 */
import type { ReactNode } from 'react';
import { useRef, useCallback, useEffect } from 'react';
import { useLayoutStore } from '../../stores/layoutStore';

interface Props {
  children: ReactNode;
}

export function SecondarySidebar({ children }: Props) {
  const visible = useLayoutStore((s) => s.secondarySidebarVisible);
  const width = useLayoutStore((s) => s.secondarySidebarWidth);
  const setWidth = useLayoutStore((s) => s.setSecondarySidebarWidth);
  const resizing = useRef(false);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    resizing.current = true;
  }, []);

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!resizing.current) return;
      setWidth(e.clientX - 48); // minus ActivityBar width
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
  }, [setWidth]);

  if (!visible) return null;

  return (
    <div
      className="flex-shrink-0 bg-bg-secondary border-r border-border flex flex-col overflow-hidden"
      style={{ width }}
    >
      {children}
      {/* Resize handle */}
      <div
        onMouseDown={onMouseDown}
        className="absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-accent/30 transition-colors z-10"
        style={{ marginRight: -1 }}
      />
    </div>
  );
}
