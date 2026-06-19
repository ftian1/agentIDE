/** Hook that provides a TerminalApi instance (singleton). */
import { useRef } from 'react';
import { createTerminalApi, type TerminalApi } from '../api/terminalApi';

export function useTerminalApi(): TerminalApi {
  const ref = useRef<TerminalApi | null>(null);
  if (!ref.current) {
    ref.current = createTerminalApi();
  }
  return ref.current;
}
