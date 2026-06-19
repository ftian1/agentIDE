/**
 * Protocol helpers for working with message types in the frontend.
 * These mirror shared-protocol message types but with frontend-friendly APIs.
 */

import type { ProtocolMessage, MessageType } from 'shared-types';

/** Type guard: check if a message is of a specific type. */
export function isMessageType<T extends ProtocolMessage>(
  msg: ProtocolMessage,
  type: T['type'],
): msg is T {
  return msg.type === type;
}

/** Get a display-friendly label for a message type. */
export function messageTypeLabel(type: MessageType): string {
  const labels: Record<MessageType, string> = {
    hello: 'Hello',
    hello_ack: 'Hello ACK',
    error: 'Error',
    goodbye: 'Goodbye',
    spawn_session: 'Spawn Session',
    spawn_session_ack: 'Spawn ACK',
    spawn_session_nack: 'Spawn NACK',
    close_session: 'Close Session',
    close_session_ack: 'Close ACK',
    terminal_data: 'Terminal Data',
    terminal_input: 'Terminal Input',
    terminal_resize: 'Terminal Resize',
    ack: 'ACK',
    pause: 'Pause',
    resume: 'Resume',
    session_event: 'Session Event',
    probe_request: 'Probe Request',
    probe_response: 'Probe Response',
    install_request: 'Install Request',
    install_progress: 'Install Progress',
    install_complete: 'Install Complete',
    ping: 'Ping',
    pong: 'Pong',
  };
  return labels[type] ?? type;
}
